// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! RetroTerm — emulador de terminal personal con estética retro.
//!
//! Autor: Alfonso García Tejeo <agtejeo@ugr.es>. Licencia: GPL-3.0-or-later.
//!
//! main.rs ahora solo arranca la aplicación; el trabajo está repartido en módulos:
//! - crt.rs              efectos de pantalla: líneas de barrido, viñeteado, esquinas
//! - paleta.rs           los 256 colores convertidos a tonos de fósforo
//! - tema.rs             colores, fuente, cursor y estilos de las ventanas
//! - perfil.rs           perfiles de servidor y su fichero en disco
//! - llavero.rs          contraseñas cifradas en KWallet / GNOME Keyring
//! - acerca.rs           ventana "Acerca de" (versión, autor, licencia)
//! - actividad.rs        aviso animado en pestañas en segundo plano con novedades
//! - ajustes.rs          fuente, tamaño, fósforo, paleta y efectos (ajustes.toml)
//! - preferencias.rs     la ventana de ajustes
//! - bloqueo.rs          bloqueo con salvapantallas y contraseña de sesión
//! - osciloscopio.rs     el salvapantallas: curvas de Lissajous con persistencia
//! - bloc.rs             bloc de notas (panel de texto libre, solo en memoria)
//! - sesion.rs           comprobar la contraseña de sesión con PAM
//! - pam_ffi.rs          conexión directa (FFI) con la librería de C libpam
//! - boveda.rs           entradas de root del fichero KeePass (solo en memoria)
//! - desbloqueo.rs       ventana de la contraseña maestra de KeePass
//! - autorespuesta.rs    "espera este texto -> escribe aquello"
//! - mantener.rs         "Mantener sesión abierta" (Intro en prompt vacío contra TMOUT)
//! - lanzador.rs         ventana de inicio (elegir servidor y conectar)
//! - ventana_terminal.rs ventanas de terminal (local o SSH)

mod acerca;
mod actividad;
mod ajustes;
mod autorespuesta;
mod bloc;
mod bloqueo;
mod boveda;
mod crt;
mod desbloqueo;
mod halo;
mod invasion;
mod lanzador;
mod llavero;
mod lluvia;
mod mantener;
mod osciloscopio;
mod paleta;
mod pam_ffi;
mod pegado;
mod perfil;
mod preferencias;
mod salvapantallas;
mod sesion;
mod sobrevuelo;
mod tema;
mod ventana_terminal;

use gtk::glib;
use gtk::prelude::*;
use vte4::gtk;

const APP_ID: &str = "es.personal.RetroTerm";

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();

    // "startup" ocurre una vez al arrancar; "activate", cada vez que se
    // abre la aplicación (si ya estaba abierta, abre otra ventana de inicio).
    app.connect_startup(|_| {
        tema::instalar_estilos();
        // Icono de todas las ventanas: el que instala el RPM en
        // /usr/share/icons/hicolor/scalable/apps/es.personal.RetroTerm.svg
        gtk::Window::set_default_icon_name(APP_ID);
    });
    app.connect_activate(lanzador::mostrar);

    app.run()
}
