// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! La "bóveda": las contraseñas de root sacadas del fichero KeePass (.kdbx).
//!
//! (No se llama keepass.rs para no confundirse con el crate `keepass`.)
//!
//! Seguridad:
//! - El .kdbx se descifra con la contraseña maestra, que NO se guarda en
//!   ningún sitio: se usa una vez y se descarta.
//! - De todo el fichero solo nos quedamos, EN MEMORIA, con las entradas
//!   cuyo usuario es `root`. Nada descifrado se escribe en disco.
//! - "Bloquear" (o cerrar RetroTerm) borra esas entradas de la memoria.
//! - Bloqueo automático: tras `keepass_auto_min` minutos SIN USARLA (ajuste,
//!   0 = nunca) la bóveda se bloquea sola. También al bloquear RetroTerm.
//!
//! Cada entrada se identifica por su ruta de grupos + título, p. ej.
//! "WEB / NODOS". Un perfil guarda esa ruta (nunca la contraseña), así que si
//! cambias la contraseña en KeePassXC, RetroTerm usa la nueva sin más.

use std::cell::RefCell;
use std::fs::File;
use std::path::Path;

use keepass::db::{Group, Node};
use keepass::{Database, DatabaseKey};
use vte4::gtk::glib;

use crate::ajustes;

/// Una entrada de root del .kdbx.
#[derive(Clone)]
pub struct EntradaRoot {
    /// Grupos + título: "WEB / NODOS".
    pub ruta: String,
    /// Servidor deducido de la URL o, si no hay, del título: "oc".
    pub host: String,
    /// Privado: fuera de este módulo solo se lee con `clave_root`.
    clave: String,
}

thread_local! {
    /// `None` = bóveda bloqueada. `Some(lista)` = desbloqueada.
    static ENTRADAS: RefCell<Option<Vec<EntradaRoot>>> = RefCell::new(None);

    /// El temporizador del bloqueo automático, si hay uno en marcha.
    /// `glib::SourceId` es el "resguardo" que devuelve GLib al programar algo:
    /// sirve para cancelarlo con `remove()` antes de que salte.
    static TEMPORIZADOR: RefCell<Option<glib::SourceId>> = RefCell::new(None);

    /// A quién avisar cuando la bóveda se bloquea (la ventana de conexiones,
    /// para cambiar su botón a «Desbloquear…»). Solo hay un hueco: hay como
    /// mucho una ventana de conexiones abierta.
    ///
    /// `Box<dyn Fn()>`: "alguna función sin parámetros" guardada en el
    /// montón (`Box`), porque cada closure tiene su propio tipo y tamaño.
    static AL_BLOQUEAR: RefCell<Option<Box<dyn Fn()>>> = RefCell::new(None);
}

/// Abre y descifra el .kdbx, y devuelve sus entradas de root.
///
/// Esta función NO toca la bóveda en memoria: solo lee. Así se puede ejecutar
/// en otro hilo (descifrar tarda un poco) sin bloquear la ventana.
pub fn leer(ruta: &Path, maestra: &str) -> Result<Vec<EntradaRoot>, String> {
    let mut fichero = File::open(ruta)
        .map_err(|error| format!("No se pudo abrir {}: {error}", ruta.display()))?;
    let clave = DatabaseKey::new().with_password(maestra);
    let base = Database::open(&mut fichero, clave)
        .map_err(|error| format!("No se pudo descifrar (¿contraseña incorrecta?): {error}"))?;

    let mut entradas = Vec::new();
    // `&mut Vec::new()` = una lista vacía, prestada para modificarla.
    recorrer(&base.root, &mut Vec::new(), &mut entradas, true);
    entradas.sort_by(|a, b| a.ruta.cmp(&b.ruta));
    Ok(entradas)
}

/// Recorre un grupo y todos sus subgrupos (una función RECURSIVA: se llama a
/// sí misma para cada subgrupo).
///
/// `camino` lleva los nombres de los grupos por los que vamos bajando.
fn recorrer(grupo: &Group, camino: &mut Vec<String>, salida: &mut Vec<EntradaRoot>, es_raiz: bool) {
    // El grupo raíz (p. ej. "Raíz") no lo ponemos en la ruta.
    if !es_raiz {
        camino.push(grupo.name.clone());
    }

    for nodo in &grupo.children {
        match nodo {
            Node::Group(subgrupo) => recorrer(subgrupo, camino, salida, false),
            Node::Entry(entrada) => {
                // `.map(str::trim)` = si hay usuario, quítale los espacios.
                if entrada.get_username().map(str::trim) == Some("root") {
                    let titulo = entrada.get_title().unwrap_or("").trim().to_string();
                    let mut partes = camino.clone();
                    partes.push(titulo.clone());
                    salida.push(EntradaRoot {
                        ruta: partes.join(" / "),
                        host: deducir_host(entrada.get_url().unwrap_or(""), &titulo),
                        clave: entrada.get_password().unwrap_or("").to_string(),
                    });
                }
            }
        }
    }

    if !es_raiz {
        camino.pop(); // al salir del grupo, lo quitamos del camino
    }
}

/// "http://proxy.ejemplo.org/algo" -> "proxy.ejemplo.org"; sin URL útil, el título:
/// "NODOS" -> "nodos".
fn deducir_host(url: &str, titulo: &str) -> String {
    let url = url.trim();
    // Quitamos "esquema://" si lo hay.
    let sin_esquema = match url.split_once("://") {
        Some((_esquema, resto)) => resto,
        None => url,
    };
    // Nos quedamos con lo que hay antes del primer '/', ':' o espacio.
    let host = sin_esquema
        .split(|c: char| c == '/' || c == ':' || c.is_whitespace())
        .next()
        .unwrap_or("");
    let parece_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');

    if parece_host {
        host.to_lowercase()
    } else {
        // Primera palabra del título: "SWITCH1 y 2" -> "switch1".
        titulo
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    }
}

/// Los "cambios" que hay que aplicar a las contraseñas guardadas en
/// KeePass antes de escribirlas: cada par es (carácter original, nuevo).
/// Para cambiar las reglas, edita SOLO esta tabla.
pub const CAMBIOS: [(char, char); 4] = [('e', '6'), ('a', ','), ('o', '.'), ('i', 'I')];

/// Aplica la tabla CAMBIOS a una contraseña, carácter a carácter.
///
/// Ojo: lo hacemos de una pasada con `.chars().map(...)` y no con varios
/// `.replace()` seguidos. Con replace en cadena, un carácter ya cambiado
/// podría volver a cambiarse con la regla siguiente (si una regla
/// produjera la letra que otra regla busca). Así, cada carácter original
/// se mira UNA vez.
pub fn aplicar_cambios(clave: &str) -> String {
    clave
        .chars()
        .map(|c| {
            CAMBIOS
                .iter()
                .find(|(original, _)| *original == c)
                .map(|(_, nuevo)| *nuevo)
                .unwrap_or(c) // si no hay regla para c, se queda igual
        })
        .collect() // un iterador de char se puede recoger en un String
}

/// Guarda en memoria las entradas leídas: la bóveda queda desbloqueada.
pub fn desbloquear(entradas: Vec<EntradaRoot>) {
    ENTRADAS.with(|contenido| *contenido.borrow_mut() = Some(entradas));
    reiniciar_temporizador();
}

/// Olvida todas las entradas: la bóveda queda bloqueada.
pub fn bloquear() {
    parar_temporizador();
    // `take()` saca el valor y deja `None` en su sitio: así sabemos si
    // estaba abierta (y solo entonces avisamos).
    let estaba_abierta = ENTRADAS.with(|contenido| contenido.borrow_mut().take().is_some());
    if estaba_abierta {
        // El préstamo de ENTRADAS ya se soltó (acabó el `with`): el aviso
        // puede volver a consultar la bóveda sin chocar con él.
        AL_BLOQUEAR.with(|aviso| {
            if let Some(aviso) = &*aviso.borrow() {
                aviso();
            }
        });
    }
}

/// Registra la función a la que avisar cuando la bóveda se bloquee
/// (sustituye a la anterior, si la había).
pub fn al_bloquear(aviso: impl Fn() + 'static) {
    AL_BLOQUEAR.with(|hueco| *hueco.borrow_mut() = Some(Box::new(aviso)));
}

/// Vuelve a contar desde cero los minutos del bloqueo automático. Se llama
/// al desbloquear y cada vez que se USA una contraseña de root.
pub fn reiniciar_temporizador() {
    parar_temporizador();
    let minutos = ajustes::actuales().keepass_auto_min;
    if minutos == 0 || !abierta() {
        return; // 0 = nunca; y si está bloqueada, no hay nada que vigilar
    }
    // `timeout_add_seconds_local_once`: ejecuta el closure UNA vez, pasado
    // ese tiempo, en el hilo de GTK.
    let id = glib::timeout_add_seconds_local_once(minutos * 60, || {
        // Ya ha saltado: GLib lo da por terminado. Hay que olvidar el
        // resguardo SIN llamar a `remove()` (fallaría: ya no existe).
        TEMPORIZADOR.with(|t| t.borrow_mut().take());
        eprintln!("KeePass bloqueado automáticamente por inactividad.");
        bloquear();
    });
    TEMPORIZADOR.with(|t| *t.borrow_mut() = Some(id));
}

/// Cancela el temporizador si está en marcha.
fn parar_temporizador() {
    // `take()` lo saca del hueco; si había uno, lo cancelamos.
    if let Some(id) = TEMPORIZADOR.with(|t| t.borrow_mut().take()) {
        id.remove();
    }
}

pub fn abierta() -> bool {
    ENTRADAS.with(|contenido| contenido.borrow().is_some())
}

/// Las entradas disponibles como (ruta, host), SIN contraseñas.
pub fn lista() -> Vec<(String, String)> {
    ENTRADAS.with(|contenido| match &*contenido.borrow() {
        Some(entradas) => entradas
            .iter()
            .map(|e| (e.ruta.clone(), e.host.clone()))
            .collect(),
        None => Vec::new(),
    })
}

/// La contraseña de root de una entrada, si la bóveda está abierta y existe.
/// Usarla cuenta como actividad: reinicia el bloqueo automático.
pub fn clave_root(ruta: &str) -> Option<String> {
    let clave = ENTRADAS.with(|contenido| {
        contenido
            .borrow()
            .as_ref()? // `?` también funciona con Option: si es None, devuelve None
            .iter()
            .find(|e| e.ruta == ruta)
            .map(|e| e.clave.clone())
    });
    if clave.is_some() {
        reiniciar_temporizador();
    }
    clave
}
