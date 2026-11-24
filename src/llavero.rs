// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Contraseñas en el llavero del sistema.
//!
//! Usamos el estándar "Secret Service" de freedesktop.org, que implementan
//! KWallet (KDE) y GNOME Keyring. El llavero cifra los secretos y los
//! desbloquea con tu sesión: nosotros nunca escribimos una contraseña en disco.
//!
//! Cada contraseña se guarda con:
//!   servicio = "retroterm"
//!   usuario  = "usuario@servidor:puerto"
//! (en KWallet aparece dentro de la carpeta "Secret Service").

use keyring::{Entry, Error};

use crate::perfil::Perfil;

const SERVICIO: &str = "retroterm";

/// La "ficha" del llavero que corresponde a un perfil.
fn entrada(perfil: &Perfil) -> Result<Entry, Error> {
    // Los perfiles locales no tienen servidor propio (todos serían
    // "localhost"): su contraseña se guarda con el nombre del perfil.
    let cuenta = if perfil.es_local() {
        format!("local:{}", perfil.nombre)
    } else {
        format!("{}@{}:{}", perfil.usuario, perfil.host, perfil.puerto)
    };
    Entry::new(SERVICIO, &cuenta)
}

/// Lee la contraseña guardada.
///
/// Devuelve un `Result` que contiene un `Option`: dos preguntas distintas.
/// - `Err(...)`      -> el llavero falló (no hay llavero, está bloqueado...)
/// - `Ok(None)`      -> el llavero funciona, pero no hay contraseña guardada
/// - `Ok(Some(c))`   -> aquí está la contraseña
pub fn leer(perfil: &Perfil) -> Result<Option<String>, Error> {
    // El `?` tras entrada(perfil) propaga el error si no se pudo crear la ficha.
    match entrada(perfil)?.get_password() {
        Ok(clave) => Ok(Some(clave)),
        Err(Error::NoEntry) => Ok(None), // "no existe" no es un fallo para nosotros
        Err(error) => Err(error),
    }
}

pub fn guardar(perfil: &Perfil, clave: &str) -> Result<(), Error> {
    entrada(perfil)?.set_password(clave)
}

// ---- Contraseña maestra de KeePass (opcional, la casilla viene desmarcada).
//
// Se guarda aparte, con servicio "retroterm-keepass" y como "usuario" la
// ruta del fichero .kdbx. OJO: con ella guardada, el .kdbx entero queda
// protegido solo por tu sesión (el llavero se abre al iniciar sesión).

const SERVICIO_KEEPASS: &str = "retroterm-keepass";

pub fn leer_maestra(fichero: &str) -> Result<Option<String>, Error> {
    match Entry::new(SERVICIO_KEEPASS, fichero)?.get_password() {
        Ok(clave) => Ok(Some(clave)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn guardar_maestra(fichero: &str, clave: &str) -> Result<(), Error> {
    Entry::new(SERVICIO_KEEPASS, fichero)?.set_password(clave)
}

pub fn borrar_maestra(fichero: &str) -> Result<(), Error> {
    match Entry::new(SERVICIO_KEEPASS, fichero)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Borra la contraseña. Si no existía, también es un éxito.
pub fn borrar(perfil: &Perfil) -> Result<(), Error> {
    match entrada(perfil)?.delete_credential() {
        // `|` en un patrón = "o". Cualquiera de los dos casos vale.
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(error),
    }
}
