// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Comprobar la contraseña de tu sesión de Linux, con PAM.
//!
//! PAM ("Pluggable Authentication Modules") es el sistema con el que Linux
//! comprueba contraseñas: lo usan el login, `su`, `sudo` y la pantalla de
//! bloqueo de KDE. Nosotros le pasamos el usuario y la contraseña escrita, y
//! PAM responde sí o no. RetroTerm nunca ve tu contraseña guardada en ningún
//! sitio: Linux solo guarda un hash irreversible en /etc/shadow.
//!
//! Qué reglas aplica PAM lo decide un fichero de /etc/pam.d/ (el "servicio").
//! Usamos el primero de estos que exista (solo uno; ver `servicios_disponibles`):
//!   1. "retroterm" -> el que instalan los paquetes RPM / .deb.
//!   2. "login"     -> el del inicio de sesión, que existe en todas partes
//!                     (es el que se usa con `cargo run`). En macOS también
//!                     existe (/etc/pam.d/login) y comprueba la contraseña del
//!                     Mac con `pam_opendirectory`.
//!
//! NO usamos "kde" (el de la pantalla de bloqueo de KDE): está pensado para
//! su propio programa y rechaza a los demás. Peor aún, cada rechazo cuesta
//! una espera de seguridad de varios segundos antes de probar el siguiente.

use std::path::Path;

use vte4::gtk::glib;

use crate::pam_ffi;

const SERVICIOS: [&str; 2] = ["retroterm", "login"];
/// Dónde pueden estar los ficheros de PAM según la distribución
/// (openSUSE Tumbleweed usa también /usr/lib/pam.d y /usr/etc/pam.d).
const CARPETAS_PAM: [&str; 3] = ["/etc/pam.d", "/usr/lib/pam.d", "/usr/etc/pam.d"];

/// El servicio PAM que usaremos: el PRIMERO de `SERVICIOS` que exista.
///
/// Solo uno, a propósito: antes, si "retroterm" rechazaba la contraseña
/// (p. ej. una errata), se probaba también "login", cuya pila común empieza
/// por `pam_fprintd` (lector de huellas) y esperaba ~30 s antes de decir
/// que no. Una contraseña mala no va a ser buena con otro servicio.
fn servicios_disponibles() -> Vec<&'static str> {
    // `find` devuelve un Option: Some(el primero que cumple) o None.
    let elegido = SERVICIOS.iter().copied().find(|servicio| {
        CARPETAS_PAM
            .iter()
            .any(|carpeta| Path::new(carpeta).join(servicio).exists())
    });
    // `unwrap_or`: si no hay ninguno (None), "login" de todas formas.
    vec![elegido.unwrap_or("login")]
}

/// Tu nombre de usuario de Linux.
pub fn usuario() -> String {
    // `user_name()` devuelve un OsString (texto del sistema operativo, que en
    // teoría podría no ser UTF-8). `to_string_lossy` lo convierte a texto.
    glib::user_name().to_string_lossy().into_owned()
}

/// ¿Es `clave` la contraseña de tu sesión?
///
/// Usa el servicio PAM elegido por `servicios_disponibles`.
/// Cada intento se anota en la consola (stderr) con el código de error real
/// de PAM, para poder diagnosticar si algo falla.
///
/// OJO: puede tardar unos segundos (tras un fallo, PAM espera a propósito
/// para frenar a quien pruebe contraseñas). Por eso se llama desde otro hilo.
pub fn comprobar(clave: &str) -> Result<(), String> {
    let usuario = usuario();
    let mut ultimo_error = String::new();

    for servicio in servicios_disponibles() {
        // `pam_ffi::autenticar` es nuestra conexión directa con libpam.so.
        match pam_ffi::autenticar(servicio, &usuario, clave) {
            Ok(()) => {
                eprintln!("PAM [{servicio}] usuario {usuario}: aceptada.");
                return Ok(());
            }
            Err(error) => {
                eprintln!("PAM [{servicio}] usuario {usuario}: rechazada: {error}");
                ultimo_error = format!("{servicio}: {error}");
            }
        }
    }
    Err(format!("Contraseña no aceptada ({ultimo_error})."))
}
