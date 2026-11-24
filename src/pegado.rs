// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Aviso al pegar varias líneas en una shell de root.
//!
//! Al pegar en una terminal, cada salto de línea es un Intro: varias líneas
//! se EJECUTAN una tras otra, al momento. Si la opción está activada en
//! Ajustes y el cursor está en un prompt de root (acaba en `#`), antes de
//! pegar texto con saltos de línea se pregunta qué hacer:
//!
//!   [Cancelar]   [Pegar en una sola línea]   [Pegar]
//!
//! Con la opción desactivada (lo normal), pegar funciona como siempre.

use std::rc::Rc;

use gtk::{gdk, glib};
use vte4::gtk;
use vte4::prelude::*;
use vte4::Terminal;

use crate::ajustes;
use crate::autorespuesta;

/// De dónde se pega.
pub enum Origen {
    /// El portapapeles normal (Ctrl+Mayús+V, Mayús+Insert, menú «Pegar»).
    Portapapeles,
    /// La selección primaria (botón central del ratón).
    Primaria,
}

/// Cuántas líneas del texto se muestran en el aviso, como mucho.
const LINEAS_VISTA: usize = 8;
/// Cuántos caracteres de cada línea se muestran, como mucho.
const ANCHO_VISTA: usize = 70;

/// ¿Hay que vigilar un pegado en esta terminal ahora mismo? Solo si la
/// opción está activada y el cursor está en un prompt de root vacío.
pub fn hay_que_vigilar(terminal: &Terminal) -> bool {
    ajustes::actuales().aviso_pegar_root && autorespuesta::en_prompt_root(terminal)
}

/// Pega en la terminal desde `origen`, preguntando antes si hace falta.
pub fn pegar(terminal: &Terminal, origen: Origen) {
    if !hay_que_vigilar(terminal) {
        // Lo de siempre: que VTE pegue directamente.
        match origen {
            Origen::Portapapeles => terminal.paste_clipboard(),
            Origen::Primaria => terminal.paste_primary(),
        }
        return;
    }

    // Hay que mirar el texto ANTES de pegarlo. Leer el portapapeles es
    // ASÍNCRONO (el texto puede venir de otro programa y tardar un poco):
    // `spawn_future_local` ejecuta el bloque `async` en el bucle de GTK, y
    // `.await` espera al resultado SIN congelar la interfaz.
    let portapapeles: gdk::Clipboard = match origen {
        Origen::Portapapeles => terminal.clipboard(),
        Origen::Primaria => terminal.primary_clipboard(),
    };
    let terminal = terminal.clone();
    glib::spawn_future_local(async move {
        // `Ok(Some(texto))`: se pudo leer y había texto. En cualquier otro
        // caso (error, vacío, una imagen…) no hay nada que pegar.
        let Ok(Some(texto)) = portapapeles.read_text_future().await else {
            return;
        };
        let texto = texto.to_string();
        if texto.contains('\n') || texto.contains('\r') {
            preguntar(&terminal, texto);
        } else {
            // Una sola línea sin Intro: no se ejecuta nada, se pega sin más.
            terminal.paste_text(&texto);
        }
    });
}

/// La ventana de aviso, con la vista previa del texto y los tres botones.
fn preguntar(terminal: &Terminal, texto: String) {
    let padre = terminal.root().and_downcast::<gtk::Window>();
    let ventana = gtk::Window::builder()
        .title("RetroTerm — Pegar en una shell de root")
        .modal(true) // mientras esté abierta, la ventana de terminales no responde
        .resizable(false)
        .build();
    ventana.set_transient_for(padre.as_ref()); // encima de la ventana de terminales
    ventana.add_css_class("retro");
    // Que cuente como ventana de la aplicación (como las demás).
    if let Some(app) = padre.as_ref().and_then(|padre| padre.application()) {
        ventana.set_application(Some(&app));
    }

    // `lines()` separa por saltos de línea (y quita el `\r` de "\r\n").
    let lineas: Vec<&str> = texto.lines().collect();
    let explicacion = if lineas.len() == 1 {
        String::from("Vas a pegar 1 línea que acaba en salto de línea:\nse ejecutará al momento.")
    } else {
        format!(
            "Vas a pegar {} líneas. Cada una se ejecutará\nen cuanto se pegue.",
            lineas.len()
        )
    };

    // Vista previa: las primeras líneas, recortadas si son muy largas.
    let mut vista: Vec<String> = lineas
        .iter()
        .take(LINEAS_VISTA)
        .map(|linea| {
            // `chars().count()`: contamos LETRAS, no bytes (las tildes ocupan 2).
            if linea.chars().count() > ANCHO_VISTA {
                let recorte: String = linea.chars().take(ANCHO_VISTA).collect();
                format!("  {recorte}…")
            } else {
                format!("  {linea}")
            }
        })
        .collect();
    if lineas.len() > LINEAS_VISTA {
        vista.push(format!("  … y {} líneas más", lineas.len() - LINEAS_VISTA));
    }

    let etiqueta = gtk::Label::builder().label(explicacion).xalign(0.0).build();
    let previa = gtk::Label::builder()
        .label(vista.join("\n"))
        .xalign(0.0)
        .build();
    previa.add_css_class("ayuda");

    let boton_cancelar = gtk::Button::with_label("Cancelar");
    let boton_una_linea = gtk::Button::with_label("Pegar en una sola línea");
    let boton_pegar = gtk::Button::with_label("Pegar");
    let botones = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    botones.set_halign(gtk::Align::End);
    botones.append(&boton_cancelar);
    botones.append(&boton_una_linea);
    botones.append(&boton_pegar);

    let contenido = gtk::Box::new(gtk::Orientation::Vertical, 14);
    contenido.set_margin_top(20);
    contenido.set_margin_bottom(20);
    contenido.set_margin_start(24);
    contenido.set_margin_end(24);
    contenido.append(&etiqueta);
    contenido.append(&previa);
    contenido.append(&botones);
    ventana.set_child(Some(&contenido));

    // El texto lo comparten dos botones: `Rc<str>` es un texto compartido
    // de solo lectura (clonar el Rc no copia el texto).
    //
    // ORDEN IMPORTANTE: `lineas` son trozos PRESTADOS de `texto` (`&str`).
    // Primero usamos `lineas` para `una_linea`, y solo después MOVEMOS
    // `texto` al Rc. Al revés, Rust daría el error E0505: "no puedes mover
    // `texto` porque está prestado" (los trozos apuntarían a la nada).
    let una_linea: Rc<str> = Rc::from(lineas.join(" ").trim_end());
    let texto: Rc<str> = Rc::from(texto);

    // Los tres botones hacen casi lo mismo: pegar algo (o nada) y cerrar.
    // `Option<Rc<str>>`: None = Cancelar.
    for (boton, que_pegar) in [
        (&boton_cancelar, None),
        (&boton_una_linea, Some(una_linea)),
        (&boton_pegar, Some(texto)),
    ] {
        // Referencia débil a la ventana: el closure vive en un botón de la
        // propia ventana (una normal formaría un ciclo; ver CLAUDE.md).
        let ventana = ventana.downgrade();
        let terminal = terminal.clone();
        boton.connect_clicked(move |_| {
            if let Some(texto) = &que_pegar {
                // `paste_text` pega como si fuera del portapapeles (respeta el
                // «bracketed paste» si la shell lo tiene activado).
                terminal.paste_text(texto);
            }
            if let Some(ventana) = ventana.upgrade() {
                ventana.close();
            }
        });
    }

    // Escape = Cancelar.
    let teclas = gtk::EventControllerKey::new();
    {
        let ventana = ventana.downgrade();
        teclas.connect_key_pressed(move |_, tecla, _, _| {
            if tecla == gdk::Key::Escape {
                if let Some(ventana) = ventana.upgrade() {
                    ventana.close();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    ventana.add_controller(teclas);

    // Se cierre como se cierre, el foco vuelve a la terminal.
    {
        let terminal = terminal.clone();
        ventana.connect_close_request(move |_| {
            terminal.grab_focus();
            glib::Propagation::Proceed // y que se cierre, claro
        });
    }

    // «Cancelar» es el botón por defecto y tiene el foco: un Intro por
    // inercia NO pega nada.
    ventana.set_default_widget(Some(&boton_cancelar));
    ventana.present();
    GtkWindowExt::set_focus(&ventana, Some(&boton_cancelar));
}
