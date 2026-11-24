// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! El halo del fósforo: un resplandor suave alrededor de cada letra, como el
//! del cristal de un monitor CRT.
//!
//! VTE no admite sombras de texto, así que lo hacemos con un WIDGET PROPIO,
//! `Halo`, que envuelve a la terminal y, al pintarse, la dibuja DOS veces:
//!
//!   1. normal (nítida);
//!   2. desenfocada y algo transparente, MEZCLADA encima como luz (modo
//!      «Screen»: solo aclara). Alrededor de cada letra queda un resplandor
//!      de su color; el fondo, que es oscuro, apenas cambia.
//!
//! GTK 4 no pinta píxeles directamente: cada widget describe su dibujo como
//! un árbol de «nodos» (GSK) que luego la tarjeta gráfica combina. Desenfocar
//! (`push_blur`) o mezclar (`push_blend`) son nodos más de ese árbol, así que
//! el efecto no toca a VTE ni al texto: teclado, ratón, selección… todo sigue
//! igual, porque el Halo solo cambia CÓMO se pinta su hijo.
//!
//! ─── Cómo se crea un widget propio en gtk-rs ───────────────────────────────
//! GTK está escrito en C con su propio sistema de objetos (GObject), con
//! clases y herencia. Rust no tiene herencia, así que gtk-rs lo resuelve con
//! dos piezas:
//!
//! - `mod imp`: la IMPLEMENTACIÓN. Un struct normal de Rust con los datos del
//!   widget, y los traits `ObjectImpl`, `WidgetImpl`… donde SOBREESCRIBIMOS
//!   los métodos de la clase padre que nos interesan (aquí, `snapshot`).
//! - `glib::wrapper!`: crea el tipo PÚBLICO `Halo`, el que usa el resto del
//!   programa. Es un "puntero" al objeto de GObject, como `gtk::Button`, y
//!   gracias a `@extends gtk::Widget` tiene todos los métodos de un widget.

use gtk::glib;
use gtk::gsk;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use vte4::gtk;

use crate::ajustes;

/// Radio del desenfoque del halo, en píxeles (cuánto se extiende el brillo).
const RADIO: f32 = 5.0;

/// La implementación del widget. Por convención se llama `imp` y es privada.
mod imp {
    // `super::*`: todo lo que se importó en el módulo de arriba.
    use super::*;

    /// Los datos del widget. No necesita ninguno: su único hijo (la terminal)
    /// lo guarda GTK, y los ajustes se leen al pintar.
    /// `#[derive(Default)]`: GObject crea el objeto con `Default::default()`.
    #[derive(Default)]
    pub struct Halo;

    /// Registro de la CLASE en GObject: su nombre, el tipo público que le
    /// corresponde y de quién hereda.
    /// `#[glib::object_subclass]` genera el código de registro por nosotros.
    #[glib::object_subclass]
    impl ObjectSubclass for Halo {
        /// Nombre del tipo en GObject (único en todo el programa; sale, p. ej.,
        /// en el inspector de GTK).
        const NAME: &'static str = "RetroTermHalo";
        type Type = super::Halo;
        type ParentType = gtk::Widget;

        /// Se ejecuta UNA vez, al registrar la clase.
        fn class_init(clase: &mut Self::Class) {
            // `BinLayout`: el hijo ocupa todo el espacio del Halo, y el Halo
            // pide el tamaño de su hijo. Sin esto habría que medir a mano.
            clase.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for Halo {
        /// Al destruir el widget hay que "desenganchar" a mano a los hijos que
        /// se pusieron con `set_parent` (los contenedores de GTK lo hacen
        /// solos; un widget propio, no).
        fn dispose(&self) {
            // `self.obj()`: desde la implementación, el objeto público (Halo).
            while let Some(hijo) = self.obj().first_child() {
                hijo.unparent();
            }
        }
    }

    impl WidgetImpl for Halo {
        /// GTK llama aquí cada vez que hay que pintar el widget. En vez de
        /// pintar, AÑADIMOS NODOS a `snapshot` (el árbol de dibujo).
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let halo = self.obj();
            let Some(hijo) = halo.first_child() else {
                return;
            };
            let ajustes = ajustes::actuales();
            if !ajustes.crt_halo || ajustes.crt_halo_intensidad <= 0.0 {
                // Sin halo: el hijo tal cual, como haría cualquier contenedor.
                halo.snapshot_child(&hijo, snapshot);
                return;
            }

            // Cada `push_…` abre un nodo y su `pop()` lo cierra; lo que se
            // dibuja entre medias queda DENTRO de ese nodo.
            //
            // `push_blend` es especial: recoge DOS imágenes. Hasta el primer
            // `pop()`, la de abajo; hasta el segundo, la de arriba. Después
            // las mezcla con el modo indicado.
            snapshot.push_blend(gsk::BlendMode::Screen);
            // Imagen de abajo: la terminal nítida.
            halo.snapshot_child(&hijo, snapshot);
            snapshot.pop();
            // Imagen de arriba: la misma terminal, desenfocada y atenuada.
            snapshot.push_opacity(ajustes.crt_halo_intensidad);
            snapshot.push_blur(RADIO.into());
            halo.snapshot_child(&hijo, snapshot);
            snapshot.pop(); // cierra el desenfoque
            snapshot.pop(); // cierra la opacidad
            snapshot.pop(); // cierra la mezcla
        }
    }
}

// El tipo público. `@extends`: de quién hereda (y por tanto qué métodos
// tiene); `@implements`: interfaces de GTK que todo widget implementa.
glib::wrapper! {
    pub struct Halo(ObjectSubclass<imp::Halo>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Halo {
    /// Crea un Halo que envuelve a `hijo` (la terminal).
    ///
    /// `impl IsA<gtk::Widget>` = "cualquier tipo que SEA un widget".
    pub fn new(hijo: &impl IsA<gtk::Widget>) -> Self {
        // `glib::Object::new()` crea el objeto de GObject; el tipo lo deduce
        // Rust de la anotación `: Self`.
        let halo: Self = glib::Object::new();
        hijo.set_parent(&halo);
        halo
    }

    /// El widget envuelto (la terminal).
    pub fn hijo(&self) -> Option<gtk::Widget> {
        self.first_child()
    }
}
