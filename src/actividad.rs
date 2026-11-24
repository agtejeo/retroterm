// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Aviso de actividad: cuando una pestaña que NO estás viendo recibe texto,
//! su etiqueta se marca con la clase CSS "actividad". El CSS (tema.rs) le pone
//! un barrido de luz animado. Al cambiar a esa pestaña, el aviso se apaga.
//!
//! Lo que escribe el propio RetroTerm (p. ej. el Intro de "Mantener sesión
//! abierta") no debe contar: para eso está el "silencio", un instante hasta
//! el que se ignoran los cambios de pantalla de esa pestaña.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vte4::gtk;
use vte4::prelude::*;
use vte4::Terminal;

use crate::ajustes;

/// Clase CSS que lleva la etiqueta de una pestaña con novedades.
pub const CLASE: &str = "actividad";

/// "Ignorar los cambios de esta pestaña hasta este instante".
/// `type` = un alias: un nombre corto y con significado para un tipo largo.
pub type Silencio = Rc<Cell<Instant>>;

/// Empieza a vigilar `terminal`. `pagina` es su página en el cuaderno y
/// `etiqueta`, el widget de su pestaña (el que se marca).
pub fn vigilar(terminal: &Terminal, pagina: &gtk::Overlay, etiqueta: &gtk::Box) -> Silencio {
    let silencio: Silencio = Rc::new(Cell::new(Instant::now()));

    let etiqueta = etiqueta.clone();
    let pagina_debil = pagina.downgrade();
    let silencio_vigilancia = silencio.clone();
    terminal.connect_contents_changed(move |terminal| {
        if !ajustes::actuales().aviso_actividad || Instant::now() < silencio_vigilancia.get() {
            return;
        }
        let Some(pagina) = pagina_debil.upgrade() else {
            return;
        };
        let Some(cuaderno) = terminal
            .ancestor(gtk::Notebook::static_type())
            .and_downcast::<gtk::Notebook>()
        else {
            return;
        };
        // ¿Es la pestaña que se está viendo? Comparamos dos Option<u32>.
        let es_la_activa = cuaderno.current_page() == cuaderno.page_num(&pagina);
        if !es_la_activa {
            etiqueta.add_css_class(CLASE);
        }
    });

    silencio
}

/// Durante los próximos `segundos`, los cambios de esta pestaña no cuentan.
pub fn silenciar(silencio: &Silencio, segundos: u64) {
    silencio.set(Instant::now() + Duration::from_secs(segundos));
}

/// Apaga el aviso de la pestaña cuya etiqueta es `etiqueta`.
pub fn apagar(etiqueta: &impl IsA<gtk::Widget>) {
    etiqueta.remove_css_class(CLASE);
}
