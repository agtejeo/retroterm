// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! El bloc de notas: un panel de texto libre a la derecha de las terminales,
//! para preparar o retocar configuraciones, órdenes, notas…
//!
//! - Se muestra y oculta con el botón 📝 o Ctrl+Mayús+B.
//! - Está FUERA del cuaderno de pestañas (en un `gtk::Paned`, ver
//!   `ventana_terminal::crear_ventana`), así que es el mismo para todas las
//!   pestañas.
//! - Ocultarlo no borra nada: el texto vive en un `TextBuffer` GLOBAL que
//!   dura lo que dura RetroTerm. NUNCA se guarda en disco (puede contener
//!   contraseñas o configuraciones delicadas).
//!
//! En GTK, el texto de un `TextView` está en un objeto aparte, su
//! `TextBuffer`: la vista solo lo MUESTRA. Por eso podemos destruir la vista
//! (al cerrarse la ventana de terminales) y dar el mismo búfer a la vista de
//! una ventana nueva: el texto sigue ahí.

use std::cell::Cell;

use gtk::glib;
use gtk::prelude::*;
use vte4::gtk;

/// Clase CSS del panel (y para encontrarlo dentro de la ventana).
pub const CLASE: &str = "bloc";

thread_local! {
    /// El texto del bloc. `thread_local!` lo crea la primera vez que se usa
    /// (con GTK ya iniciado) y vive hasta que se cierra RetroTerm.
    static BUFER: gtk::TextBuffer = gtk::TextBuffer::new(None);

    /// Qué parte del ancho ocupa el panel (1/3 al principio). Se guarda una
    /// PROPORCIÓN y no píxeles: así sigue siendo "un tercio" aunque entre
    /// medias maximices o restaures la ventana. Si arrastras el divisor, se
    /// recuerda la tuya.
    /// `Cell`: como un RefCell pero para valores que se COPIAN (números):
    /// `get()` y `set()`, sin préstamos.
    static PROPORCION: Cell<f64> = const { Cell::new(1.0 / 3.0) };
}

/// Límites de la proporción: ni un panel ridículo ni uno que tape la terminal.
const PROPORCION_MIN: f64 = 0.15;
const PROPORCION_MAX: f64 = 0.75;

/// Crea el panel (oculto): título y área de texto con barra de desplazamiento.
pub fn crear() -> gtk::Box {
    // `BUFER.with(|b| b.clone())`: copia del "puntero" al búfer global (los
    // objetos de GTK se comparten así: clonar NO duplica el texto).
    let bufer = BUFER.with(|b| b.clone());
    let vista = gtk::TextView::builder()
        .buffer(&bufer)
        .monospace(true)
        // Partir las líneas largas por donde haga falta: el panel es estrecho.
        .wrap_mode(gtk::WrapMode::WordChar)
        .left_margin(10)
        .right_margin(10)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    let desplazable = gtk::ScrolledWindow::builder()
        .child(&vista)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let titulo = gtk::Label::builder()
        .label("▒ Bloc de notas")
        .xalign(0.0)
        .build();
    titulo.add_css_class("titulo-bloc");

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class(CLASE);
    panel.append(&titulo);
    panel.append(&desplazable);
    // Que no se pueda encoger hasta desaparecer arrastrando el divisor.
    panel.set_width_request(220);
    panel.set_visible(false);
    panel
}

/// Muestra u oculta el panel de la ventana `paned` (el divisor que separa
/// las terminales del bloc). Devuelve `true` si queda visible.
pub fn alternar(paned: &gtk::Paned) -> bool {
    // `end_child()`: lo que hay a la derecha del divisor (el panel).
    let Some(panel) = paned.end_child() else {
        return false;
    };
    if panel.is_visible() {
        // Antes de ocultarlo, recordamos qué parte ocupa: su ancho (total
        // menos la posición del divisor) entre el total.
        let total = paned.width();
        if total > 0 {
            let proporcion = (total - paned.position()) as f64 / total as f64;
            // `clamp(a, b)`: el valor, pero sin bajar de a ni pasar de b.
            PROPORCION.with(|p| p.set(proporcion.clamp(PROPORCION_MIN, PROPORCION_MAX)));
        }
        panel.set_visible(false);
        false
    } else {
        panel.set_visible(true);
        colocar_divisor(paned);
        // Y OTRA VEZ cuando GTK ya haya repartido el espacio con el panel
        // visible. Al aparecer el panel, el Paned "corrige" la posición que
        // le damos según cuánto cambió el ancho desde la última vez que vio
        // los dos lados (con el bloc oculto, ese dato está desfasado) y el
        // panel salía de cualquier tamaño. En la segunda pasada ya no hay
        // nada que corregir.
        //
        // `idle_add_local_once`: ejecutar UNA vez cuando GTK no tenga nada
        // más urgente (después de recolocar y pintar). Referencia débil por
        // si la ventana se cierra justo antes.
        let debil = paned.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(paned) = debil.upgrade() {
                colocar_divisor(&paned);
            }
        });
        // El foco NO viene al bloc: se queda en la terminal (lo decide
        // quien llama). Para escribir aquí, basta un clic en el panel.
        true
    }
}

/// Pone el divisor para que el panel ocupe la proporción recordada.
/// `position` = dónde está el divisor, contando desde la izquierda.
fn colocar_divisor(paned: &gtk::Paned) {
    let total = paned.width();
    let proporcion = PROPORCION.with(Cell::get);
    // `round()` redondea; `as i32` pasa el decimal a entero.
    let ancho_panel = (total as f64 * proporcion).round() as i32;
    paned.set_position(total - ancho_panel);
}

/// Si el foco del teclado está en el bloc, su área de texto. Sirve para que
/// Ctrl+Mayús+C/V (atajos de la ventana, pensados para la terminal) copien
/// y peguen en el bloc cuando estás escribiendo en él.
pub fn vista_con_foco(ventana: &gtk::Window) -> Option<gtk::TextView> {
    // `focus()` = el widget que tiene el foco ahora; ¿es un TextView del bloc?
    // Como `set_focus`, existe en dos traits (GtkWindowExt y RootExt): hay
    // que decir cuál con la ruta completa (ver CLAUDE.md).
    let vista = GtkWindowExt::focus(ventana).and_downcast::<gtk::TextView>()?;
    let dentro_del_bloc = vista
        .ancestor(gtk::Box::static_type())
        .is_some_and(|caja| caja.has_css_class(CLASE));
    // `then_some(x)`: si la condición es cierta, Some(x); si no, None.
    dentro_del_bloc.then_some(vista)
}
