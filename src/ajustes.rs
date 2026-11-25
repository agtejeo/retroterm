// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Ajustes del usuario: fuente, tamaño, color del fósforo, paleta y efectos CRT.
//!
//! Se guardan en ~/.config/retroterm/ajustes.toml cada vez que cambian.
//! Ejemplo:
//!
//! ```toml
//! fuente = "VT323"
//! tamano = 20
//! fosforo = "verde"
//! monocromo = true
//! crt_lineas = true
//! crt_intensidad = 0.45
//! crt_vineta = true
//! crt_esquinas = true
//! intervalo_mantener = 300
//! salvapantallas = "lluvia"
//! keepass = "/home/usuario/Documentos/claves.kdbx"
//! ```

use std::cell::RefCell;
use std::fs;
use std::io::Write;
// Extensiones de Unix para los permisos (rwx) de ficheros y carpetas: traits
// que AÑADEN métodos (`mode`, `from_mode`) a tipos de la biblioteca estándar.
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vte4::gtk::glib;

use crate::tema::{self, Tema};

/// Un `enum` es un tipo que solo puede valer UNA de varias opciones.
/// Mucho más seguro que guardar "verde" en un String: con un enum es
/// imposible tener un fósforo "verdde" mal escrito.
///
/// `Copy` = se copia solo (como un número), sin `.clone()`: es pequeño.
/// `PartialEq` = se puede comparar con `==`.
/// `rename_all = "lowercase"` = en el fichero se escribe "verde", no "Verde".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fosforo {
    Verde,
    Ambar,
    Blanco,
}

impl Fosforo {
    /// Todas las opciones, en el orden del desplegable.
    pub const TODOS: [Fosforo; 3] = [Fosforo::Verde, Fosforo::Ambar, Fosforo::Blanco];

    /// Los colores de cada fósforo. `match` sobre un enum OBLIGA a cubrir
    /// todos los casos: si mañana añades `Azul`, el compilador te avisará aquí.
    pub fn tema(self) -> &'static Tema {
        match self {
            Fosforo::Verde => &tema::FOSFORO_VERDE,
            Fosforo::Ambar => &tema::FOSFORO_AMBAR,
            Fosforo::Blanco => &tema::FOSFORO_BLANCO,
        }
    }
}

/// El efecto de la pantalla de bloqueo. En el fichero: "osciloscopio",
/// "lluvia", "invasion" o "sobrevuelo".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Salvapantallas {
    Osciloscopio,
    Lluvia,
    Invasion,
    Sobrevuelo,
}

impl Salvapantallas {
    /// Todas las opciones, en el orden del desplegable de Ajustes.
    pub const TODOS: [Salvapantallas; 4] = [
        Salvapantallas::Osciloscopio,
        Salvapantallas::Lluvia,
        Salvapantallas::Invasion,
        Salvapantallas::Sobrevuelo,
    ];

    /// El texto que se ve en el desplegable.
    pub fn nombre(self) -> &'static str {
        match self {
            Salvapantallas::Osciloscopio => "Osciloscopio (curvas de Lissajous)",
            Salvapantallas::Lluvia => "Lluvia de letras",
            Salvapantallas::Invasion => "Invasión (defensa automática)",
            Salvapantallas::Sobrevuelo => "Sobrevuelo (terreno poligonal)",
        }
    }

    /// Nombre interno del efecto (el de su hijo en el `gtk::Stack`).
    pub fn id(self) -> &'static str {
        match self {
            Salvapantallas::Osciloscopio => "osciloscopio",
            Salvapantallas::Lluvia => "lluvia",
            Salvapantallas::Invasion => "invasion",
            Salvapantallas::Sobrevuelo => "sobrevuelo",
        }
    }
}

/// La última conexión que se abrió desde la ventana de conexiones: al
/// abrirla otra vez, sale elegida en el desplegable.
///
/// Un enum puede llevar DATOS dentro de una opción: `Perfil(String)` guarda
/// el nombre del perfil. En el fichero queda así:
///   ultima_conexion = "local"              (sesión local)
///   [ultima_conexion]  perfil = "web01"     (un perfil)
/// `#[default]` marca la opción que da `Default` (aún no se ha conectado).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UltimaConexion {
    #[default]
    Ninguna,
    Local,
    Perfil(String),
}

/// `#[serde(default)]` en el struct: si al fichero le falta algún campo
/// (p. ej. uno que añadamos en el futuro), se toma de `Default`.
/// Límites del tamaño de letra (Ajustes y Ctrl + / Ctrl −).
pub const TAMANO_MIN: u32 = 8;
pub const TAMANO_MAX: u32 = 48;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Ajustes {
    pub fuente: String,
    pub tamano: u32,
    pub fosforo: Fosforo,
    /// true = paleta de fósforo; false = colores originales de los programas.
    pub monocromo: bool,
    pub crt_lineas: bool,
    /// Oscuridad de las líneas de barrido: 0.0 (nada) a 1.0 (negras).
    pub crt_intensidad: f64,
    pub crt_vineta: bool,
    pub crt_esquinas: bool,
    /// Halo del fósforo: resplandor alrededor de las letras (halo.rs).
    pub crt_halo: bool,
    /// Fuerza del halo: 0.0 (nada) a 1.0 (máxima).
    pub crt_halo_intensidad: f64,
    /// Ruta del fichero .kdbx (se recuerda tras desbloquearlo una vez).
    /// `Option`: puede no haber ninguno.
    pub keepass: Option<String>,
    /// Bloquear KeePass tras estos minutos sin usar sus contraseñas (0 = nunca).
    pub keepass_auto_min: u32,
    /// Segundos sin teclear antes de que "Mantener sesión abierta" actúe.
    /// Debe ser MENOR que el TMOUT del servidor (p. ej. 300 < 900).
    pub intervalo_mantener: u32,
    /// Barrido animado en las pestañas en segundo plano que reciben texto.
    pub aviso_actividad: bool,
    /// Botón candado y Ctrl+Mayús+L para bloquear RetroTerm.
    pub bloqueo_activado: bool,
    /// Bloquear solo tras estos minutos sin tocar RetroTerm (0 = nunca).
    pub bloqueo_auto_min: u32,
    /// Efecto de la pantalla de bloqueo. Los ajustes.toml antiguos no lo
    /// tienen: gracias a `#[serde(default)]` se toma el de `Default`.
    pub salvapantallas: Salvapantallas,
    /// Al seleccionar texto en una terminal, copiarlo también al portapapeles
    /// normal (el de Ctrl+V), no solo a la selección primaria (botón central).
    pub copiar_al_seleccionar: bool,
    /// Preguntar antes de pegar varias líneas en un prompt de root (pegado.rs).
    pub aviso_pegar_root: bool,
    /// Sonido de la terminal: la «campana» (carácter BEL) que suena, p. ej.,
    /// al borrar sin nada que borrar. Por defecto, en silencio.
    pub sonido: bool,
    /// Última conexión usada (va la última: en TOML, las tablas como
    /// `[ultima_conexion]` tienen que ir detrás de los valores sueltos).
    pub ultima_conexion: UltimaConexion,
}

/// `Default` = "cómo es un valor por defecto de este tipo". Lo implementamos
/// a mano (con `impl Trait for Tipo`) porque queremos nuestros valores.
impl Default for Ajustes {
    fn default() -> Self {
        Ajustes {
            fuente: String::from("VT323"),
            tamano: 20,
            fosforo: Fosforo::Verde,
            monocromo: true,
            crt_lineas: true,
            crt_intensidad: 0.45,
            crt_vineta: true,
            crt_esquinas: true,
            crt_halo: true,
            crt_halo_intensidad: 0.5,
            keepass: None,
            keepass_auto_min: 15,
            intervalo_mantener: 300,
            aviso_actividad: true,
            bloqueo_activado: true,
            bloqueo_auto_min: 0,
            salvapantallas: Salvapantallas::Osciloscopio,
            copiar_al_seleccionar: true,
            aviso_pegar_root: false,
            sonido: false,
            ultima_conexion: UltimaConexion::Ninguna,
        }
    }
}

impl Ajustes {
    pub fn tema(&self) -> &'static Tema {
        self.fosforo.tema()
    }
}

// Los ajustes ACTUALES, accesibles desde cualquier parte del programa.
//
// `thread_local!` crea una variable global por hilo. GTK trabaja en un solo
// hilo, así que en la práctica es "la" variable global. Rust no deja tener
// globales modificables sin más (serían peligrosas con varios hilos); esta es
// una de las formas seguras de hacerlo. Se crea la primera vez que se usa.
thread_local! {
    static ACTUALES: RefCell<Ajustes> = RefCell::new(cargar());
}

/// Una copia de los ajustes actuales.
pub fn actuales() -> Ajustes {
    // `.with(|valor| ...)` da acceso a la variable thread_local.
    ACTUALES.with(|actuales| actuales.borrow().clone())
}

/// Modifica los ajustes y los guarda en disco.
///
/// Recibe un closure que hace el cambio: `ajustes::cambiar(|a| a.tamano = 22)`.
/// `impl FnOnce(&mut Ajustes)` = "cualquier función que reciba los ajustes
/// para modificarlos" (y que se llame como mucho una vez).
pub fn cambiar(cambio: impl FnOnce(&mut Ajustes)) {
    let nuevos = ACTUALES.with(|actuales| {
        let mut actuales = actuales.borrow_mut();
        cambio(&mut actuales);
        actuales.clone()
    });
    if let Err(error) = guardar(&nuevos) {
        eprintln!("No se pudieron guardar los ajustes: {error}");
    }
}

fn ruta() -> PathBuf {
    glib::user_config_dir()
        .join("retroterm")
        .join("ajustes.toml")
}

fn cargar() -> Ajustes {
    match fs::read_to_string(ruta()) {
        Ok(texto) => toml::from_str(&texto).unwrap_or_else(|error| {
            eprintln!(
                "Aviso: {} tiene un error, uso los valores por defecto:\n{error}",
                ruta().display()
            );
            Ajustes::default()
        }),
        Err(_) => Ajustes::default(), // aún no existe
    }
}

fn guardar(ajustes: &Ajustes) -> Result<(), Box<dyn std::error::Error>> {
    escribir_privado(&ruta(), &toml::to_string_pretty(ajustes)?)?;
    Ok(())
}

/// Escribe un fichero de configuración que SOLO puede leer su dueño.
///
/// Los ficheros de ~/.config/retroterm no llevan contraseñas, pero sí
/// servidores, usuarios y guiones (y alguien podría poner un texto delicado
/// en un guion). En distribuciones donde /home/usuario es legible por todos
/// (Debian, Ubuntu), `fs::write` los dejaría en 0644: legibles por cualquier
/// usuario de la máquina. Aquí: carpeta 0700 y fichero 0600 (rw-------).
/// Lo usan ajustes.rs y perfil.rs.
pub fn escribir_privado(ruta: &Path, texto: &str) -> std::io::Result<()> {
    if let Some(carpeta) = ruta.parent() {
        fs::create_dir_all(carpeta)?;
        fs::set_permissions(carpeta, fs::Permissions::from_mode(0o700))?;
    }
    // `OpenOptions`: abrir con opciones a medida. `mode(0o600)` solo cuenta
    // al CREAR el fichero (`0o` = número en octal, como en chmod).
    let mut fichero = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true) // vaciarlo si ya existía
        .mode(0o600)
        .open(ruta)?;
    // Si ya existía (con 0644, de versiones anteriores), se corrige aquí,
    // ANTES de escribir el contenido.
    fichero.set_permissions(fs::Permissions::from_mode(0o600))?;
    fichero.write_all(texto.as_bytes())
}
