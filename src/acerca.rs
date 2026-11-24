// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! La ventana "Acerca de RetroTerm": versión, autor, licencia y créditos.
//!
//! La versión y el autor NO se escriben aquí a mano: salen de Cargo.toml con
//! la macro `env!`, que lee variables que Cargo define AL COMPILAR. Así hay un
//! solo sitio donde cambiarlos (Cargo.toml) y nunca se desincronizan.

use gtk::prelude::*;
use vte4::gtk;

/// Versión del programa, p. ej. "0.14.0" (de Cargo.toml, `version`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Autor, de Cargo.toml (`authors`).
pub const AUTOR: &str = env!("CARGO_PKG_AUTHORS");

/// Muestra el diálogo "Acerca de", encima de `padre` si se indica.
pub fn mostrar(padre: Option<&gtk::Window>) {
    let dialogo = gtk::AboutDialog::builder()
        .program_name("RetroTerm")
        .version(VERSION)
        .comments("Emulador de terminal con estética retro,\nperfiles SSH, guiones y KeePass.")
        .copyright("© 2026 Alfonso García Tejeo")
        // GTK ya trae el texto y el enlace de la GPL v3 "o posterior".
        .license_type(gtk::License::Gpl30)
        .logo_icon_name("es.personal.RetroTerm")
        .modal(true)
        .build();
    dialogo.set_transient_for(padre);
    dialogo.set_authors(&[AUTOR]);
    dialogo.add_credit_section(
        "Fuente",
        &["VT323 — Peter Hull (SIL Open Font License 1.1)"],
    );
    dialogo.add_credit_section(
        "Construido con",
        &[
            "GTK 4 y VTE (proyecto GNOME)",
            "gtk-rs / vte4-rs (bindings de Rust)",
            "keepass-rs, keyring-rs, serde, toml",
        ],
    );
    dialogo.present();
}
