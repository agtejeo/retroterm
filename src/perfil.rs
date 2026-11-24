// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Perfiles de conexión: qué son y cómo se guardan en disco.
//!
//! Se guardan en ~/.config/retroterm/perfiles.toml, que puedes abrir y
//! editar a mano. Ejemplo de contenido:
//!
//! ```toml
//! [[perfil]]
//! nombre = "servidor01"
//! host = "servidor01.midominio.es"
//! usuario = "usuario"
//! puerto = 22
//! ```
//!
//! Aquí NO se guardan contraseñas: van cifradas al llavero (llavero.rs).

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use vte4::gtk::glib;

use crate::ajustes;

/// Un servidor al que conectarse.
///
/// `#[derive(...)]` le pide al compilador que escriba código por nosotros:
/// - `Debug`: poder imprimirlo con `{:?}` para depurar.
/// - `Clone`: poder hacer copias con `.clone()`.
/// - `Serialize` / `Deserialize`: convertirlo a TOML y leerlo de TOML (serde).
///
/// `String` (a diferencia de `&str`) es un texto que nos PERTENECE: vive tanto
/// como el struct. Lo necesitamos porque los datos vienen del disco o del
/// formulario, no del código fuente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Perfil {
    pub nombre: String,
    pub host: String,
    pub usuario: String,
    /// `u16` = entero sin signo de 16 bits (0..65535), justo el rango de un puerto.
    /// Si falta en el fichero, se usa 22.
    #[serde(default = "puerto_por_defecto")]
    pub puerto: u16,
    /// Pasos a ejecutar tras conectar. Con `#[serde(default)]`, los perfiles
    /// guardados antes de existir este campo se leen con un guion vacío.
    #[serde(default)]
    pub guion: Vec<PasoGuion>,
    /// Entrada de KeePass con la contraseña de root, p. ej. "WEB / NODOS".
    /// Se usa en el guion con `{root}`. Solo la RUTA, nunca la contraseña.
    #[serde(default)]
    pub keepass: Option<String>,
    /// Aplicar los "cambios a caracteres" (boveda::CAMBIOS) a la contraseña
    /// de KeePass. Si falta en el fichero, vale true.
    #[serde(default = "verdadero")]
    pub cambios_root: bool,
    /// "Mantener sesión abierta": Intro automático en el prompt vacío tras
    /// un rato sin teclear (contra TMOUT) y ServerAliveInterval en ssh.
    #[serde(default)]
    pub mantener_sesion: bool,
    /// Perfil LOCAL: si tiene un comando, no se usa ssh. Se abre tu shell en
    /// una pestaña y se teclea este comando (p. ej. el de la VPN); después
    /// actúa el guion como en cualquier perfil. `None` = perfil SSH normal.
    /// `skip_serializing_if`: si es `None`, ni siquiera se escribe en el fichero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comando_local: Option<String>,
}

/// ¿Sirve `texto` como servidor o usuario de ssh?
///
/// SEGURIDAD: el servidor y el usuario acaban en la línea de órdenes de ssh.
/// Si uno empezara por `-` (p. ej. `-oProxyCommand=…`), ssh lo tomaría por
/// una OPCIÓN, y algunas ejecutan órdenes en tu equipo. Un perfiles.toml
/// recibido de otra persona podría traer algo así. Tampoco se admiten
/// espacios ni caracteres de control (saltos de línea, tabuladores…).
pub fn dato_ssh_valido(texto: &str) -> bool {
    !texto.is_empty()
        && !texto.starts_with('-')
        && !texto.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl Perfil {
    /// ¿Es un perfil local (con comando) en vez de una conexión SSH?
    pub fn es_local(&self) -> bool {
        self.comando_local.is_some()
    }
}

fn verdadero() -> bool {
    true
}

/// Un paso del guion: cuando la línea del cursor empiece o termine por
/// `espera`, escribir `escribe` (y pulsar Intro).
///
/// En el fichero queda así:
/// ```toml
/// [[perfil.guion]]
/// espera = "$"
/// escribe = "sudo -i"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasoGuion {
    pub espera: String,
    pub escribe: String,
}

/// Separador entre "espera" y "escribe" en el cuadro de texto del guion.
pub const SEPARADOR: &str = "=>";

/// Guion -> texto para el cuadro de edición, una línea por paso.
pub fn guion_a_texto(guion: &[PasoGuion]) -> String {
    guion
        .iter()
        .map(|paso| format!("{} {SEPARADOR} {}", paso.espera, paso.escribe))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Texto del cuadro de edición -> guion. Las líneas vacías se ignoran.
///
/// Truco de Rust: un iterador de `Result` se puede recoger (`collect`) en un
/// `Result<Vec<_>, _>`. Si todas las líneas están bien, da `Ok(lista)`; en
/// cuanto una falla, da ese `Err` y para.
pub fn texto_a_guion(texto: &str) -> Result<Vec<PasoGuion>, String> {
    texto
        .lines()
        .enumerate()
        .filter(|(_, linea)| !linea.trim().is_empty())
        .map(|(numero, linea)| {
            // `split_once` parte el texto en dos por la primera aparición.
            // Devuelve Option: None si el separador no está.
            let (espera, escribe) = linea
                .split_once(SEPARADOR)
                .ok_or_else(|| format!("Guion, línea {}: falta «{SEPARADOR}».", numero + 1))?;
            let (espera, escribe) = (espera.trim(), escribe.trim());
            if espera.is_empty() {
                return Err(format!("Guion, línea {}: falta qué esperar.", numero + 1));
            }
            Ok(PasoGuion {
                espera: espera.to_string(),
                escribe: escribe.to_string(),
            })
        })
        .collect()
}

fn puerto_por_defecto() -> u16 {
    22
}

/// La forma del fichero completo: una lista de `[[perfil]]`.
/// Es privado: el resto del programa solo ve `Vec<Perfil>`.
#[derive(Default, Serialize, Deserialize)]
struct FicheroPerfiles {
    #[serde(default)]
    perfil: Vec<Perfil>,
}

/// Ruta del fichero. `PathBuf` es a las rutas lo que `String` a los textos.
fn ruta() -> PathBuf {
    glib::user_config_dir()
        .join("retroterm")
        .join("perfiles.toml")
}

/// Lee los perfiles del disco. Si no hay fichero todavía, lista vacía.
///
/// `Vec<Perfil>` = lista dinámica (crece y encoge) de perfiles.
pub fn cargar() -> Vec<Perfil> {
    // `match` compara un valor con varios patrones; es como un switch con
    // superpoderes. Aquí: ¿se pudo leer el fichero o no?
    let texto = match fs::read_to_string(ruta()) {
        Ok(texto) => texto,
        Err(_) => return Vec::new(), // no existe aún: es la primera vez
    };

    // `::<FicheroPerfiles>` (el "turbofish") le dice a from_str qué tipo construir.
    match toml::from_str::<FicheroPerfiles>(&texto) {
        Ok(fichero) => fichero.perfil,
        Err(error) => {
            eprintln!(
                "Aviso: {} tiene un error y no se ha leído:\n{error}",
                ruta().display()
            );
            Vec::new()
        }
    }
}

/// Escribe todos los perfiles en disco.
///
/// `Result<(), Box<dyn Error>>` = "o todo bien (sin valor que devolver: `()`),
/// o un error de cualquier tipo". El operador `?` al final de una línea
/// significa: "si esto falló, devuelve el error ya; si no, sigue".
pub fn guardar(lista: &[Perfil]) -> Result<(), Box<dyn Error>> {
    let fichero = FicheroPerfiles {
        perfil: lista.to_vec(),
    };
    let texto = toml::to_string_pretty(&fichero)?;
    // Solo legible por ti (0600): ver `ajustes::escribir_privado`.
    ajustes::escribir_privado(&ruta(), &texto)?;
    Ok(())
}

/// Añade un perfil, o lo sustituye si ya hay uno con el mismo nombre.
/// Devuelve en qué posición de la lista ha quedado.
///
/// `&mut Vec<Perfil>` = préstamo MUTABLE: podemos modificar la lista del
/// que nos llama. Rust garantiza que mientras dure, nadie más la toca.
pub fn insertar(lista: &mut Vec<Perfil>, nuevo: Perfil) -> usize {
    match lista.iter().position(|p| p.nombre == nuevo.nombre) {
        Some(i) => {
            lista[i] = nuevo;
            i
        }
        None => {
            lista.push(nuevo);
            lista.len() - 1
        }
    }
}
