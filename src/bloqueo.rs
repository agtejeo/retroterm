// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Bloqueo de RetroTerm con salvapantallas (osciloscopio o lluvia de letras).
//!
//! Es una PANTALLA DE PRIVACIDAD: tapa la ventana de terminales (pestañas
//! incluidas), deja de aceptar teclado y ratón, y para volver pide la
//! contraseña de tu sesión de Linux (sesion.rs). Las sesiones siguen vivas
//! por debajo. No sustituye al bloqueo del escritorio (Meta+L): quien tenga
//! tu escritorio puede abrir otra terminal.
//!
//! Cómo está montada la ventana de terminales:
//!
//!   ventana
//!   └─ raíz: gtk::Overlay
//!      ├─ (hijo) gtk::Notebook con las pestañas
//!      └─ (encima) CAPA de bloqueo: gtk::Overlay oculto, clase "bloqueo"
//!         ├─ (hijo) salvapantallas: gtk::Stack con un DrawingArea por
//!         │  efecto (osciloscopio, lluvia); ver salvapantallas.rs
//!         └─ (encima) caja de la contraseña, clase "caja-clave" (oculta
//!            hasta que mueves el ratón o pulsas una tecla; se vuelve a
//!            ocultar con Esc o tras 10 s sin teclear)

use std::cell::Cell;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use vte4::gtk;

use crate::ajustes;
use crate::boveda;
use crate::salvapantallas;
use crate::sesion;
use crate::ventana_terminal;

thread_local! {
    static BLOQUEADO: Cell<bool> = Cell::new(false);
    /// Cuándo se bloqueó: el ratón no cuenta durante el primer segundo
    /// (al aparecer la capa, GTK puede notificar un "movimiento" falso).
    static DESDE: Cell<Instant> = Cell::new(Instant::now());
    /// Última vez que tocaste RetroTerm (para el bloqueo automático).
    static ULTIMA_ACTIVIDAD: Cell<Instant> = Cell::new(Instant::now());
    /// Última tecla pulsada con la caja de la contraseña a la vista.
    static ULTIMA_TECLA: Cell<Instant> = Cell::new(Instant::now());
    /// Sube cada vez que se muestra la caja: el temporizador que la oculta
    /// recuerda la suya y se para si ya no es la actual (así nunca hay dos
    /// temporizadores a la vez).
    static GENERACION_CAJA: Cell<u32> = const { Cell::new(0) };
}

/// Segundos sin teclear tras los que la caja de la contraseña se oculta
/// sola (y vuelve a verse solo el salvapantallas).
const ESPERA_CAJA: Duration = Duration::from_secs(10);

/// ¿Está RetroTerm bloqueado ahora mismo?
pub fn esta_bloqueado() -> bool {
    BLOQUEADO.with(Cell::get)
}

/// Crea la capa de bloqueo (oculta). Va encima del cuaderno de pestañas.
pub fn crear_capa() -> gtk::Overlay {
    let capa = gtk::Overlay::new();
    capa.add_css_class("bloqueo");
    capa.set_visible(false);
    capa.set_child(Some(&salvapantallas::nuevo()));

    let caja = gtk::Box::new(gtk::Orientation::Vertical, 10);
    caja.add_css_class("caja-clave");
    caja.set_halign(gtk::Align::Center);
    caja.set_valign(gtk::Align::Center);
    caja.set_visible(false);

    let titulo = gtk::Label::new(Some(&format!(
        "Sesión bloqueada · contraseña de «{}»",
        sesion::usuario()
    )));
    let entrada = gtk::PasswordEntry::new();
    entrada.set_width_chars(28);
    let estado = gtk::Label::new(None);
    estado.add_css_class("estado-clave");
    caja.append(&titulo);
    caja.append(&entrada);
    caja.append(&estado);
    capa.add_overlay(&caja);

    // Intro en la contraseña: comprobarla con PAM en otro hilo (puede tardar).
    {
        let estado = estado.clone();
        entrada.connect_activate(move |entrada| {
            let clave = entrada.text().to_string();
            entrada.set_text("");
            if clave.is_empty() {
                return;
            }
            estado.set_text("Comprobando…");
            entrada.set_sensitive(false);
            let (entrada, estado) = (entrada.clone(), estado.clone());
            glib::spawn_future_local(async move {
                let resultado = gio::spawn_blocking(move || sesion::comprobar(&clave)).await;
                entrada.set_sensitive(true);
                match resultado {
                    Ok(Ok(())) => {
                        estado.set_text("");
                        if let Some(ventana) = entrada.root().and_downcast::<gtk::Window>() {
                            desbloquear(&ventana);
                        }
                    }
                    Ok(Err(mensaje)) => {
                        estado.set_text(&mensaje);
                        entrada.grab_focus();
                    }
                    Err(_) => estado.set_text("Error interno al comprobar la contraseña."),
                }
            });
        });
    }
    capa
}

/// Instala en la ventana de terminales lo que vigila teclado y ratón
/// (para mostrar la contraseña y para el bloqueo automático).
pub fn instalar(ventana: &gtk::ApplicationWindow) {
    // Los closures guardan una referencia DÉBIL a la ventana (los
    // controladores viven dentro de ella: una normal formaría un ciclo).
    let ventana_debil = ventana.downgrade();

    // Teclado, en fase de CAPTURA: lo vemos antes que nadie en la ventana.
    let teclas = gtk::EventControllerKey::new();
    teclas.set_propagation_phase(gtk::PropagationPhase::Capture);
    let ventana_teclas = ventana_debil.clone();
    teclas.connect_key_pressed(move |_controlador, tecla, _codigo, _modificadores| {
        ULTIMA_ACTIVIDAD.with(|u| u.set(Instant::now()));
        if !esta_bloqueado() {
            return glib::Propagation::Proceed; // desbloqueado: no tocamos nada
        }
        let Some(ventana) = ventana_teclas.upgrade() else {
            return glib::Propagation::Stop;
        };
        let ventana: &gtk::Window = ventana.upcast_ref();
        let Some((_capa, caja, entrada)) = piezas(ventana) else {
            return glib::Propagation::Stop;
        };
        if !caja.is_visible() {
            mostrar_caja(&caja, &entrada); // la primera tecla solo "despierta"
            return glib::Propagation::Stop;
        }
        // Con la caja a la vista, cada tecla reinicia la cuenta de 10 s.
        ULTIMA_TECLA.with(|u| u.set(Instant::now()));
        if tecla == gdk::Key::Escape {
            // Volver al salvapantallas (salvo mientras se comprueba).
            if entrada.is_sensitive() {
                ocultar_caja(&caja, &entrada);
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed // el resto va a la casilla de la contraseña
    });
    ventana.add_controller(teclas);

    // Ratón: cualquier movimiento sobre la ventana.
    let raton = gtk::EventControllerMotion::new();
    let ventana_raton = ventana_debil.clone();
    raton.connect_motion(move |_controlador, _x, _y| {
        ULTIMA_ACTIVIDAD.with(|u| u.set(Instant::now()));
        let recien_bloqueado = DESDE.with(|d| d.get().elapsed() < Duration::from_secs(1));
        if !esta_bloqueado() || recien_bloqueado {
            return;
        }
        if let Some(ventana) = ventana_raton.upgrade() {
            if let Some((_capa, caja, entrada)) = piezas(ventana.upcast_ref()) {
                if !caja.is_visible() {
                    mostrar_caja(&caja, &entrada);
                }
            }
        }
    });
    ventana.add_controller(raton);

    // Bloqueo automático: cada 10 s, ¿llevas más de N minutos sin tocar nada?
    // Con la referencia débil, si la ventana se cierra el temporizador se detiene.
    glib::timeout_add_seconds_local(10, move || {
        let Some(ventana) = ventana_debil.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let ajustes = ajustes::actuales();
        let minutos = ajustes.bloqueo_auto_min;
        if ajustes.bloqueo_activado && minutos > 0 && !esta_bloqueado() {
            let inactivo = ULTIMA_ACTIVIDAD.with(|u| u.get().elapsed());
            if inactivo >= Duration::from_secs(minutos as u64 * 60) {
                bloquear(ventana.upcast_ref());
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Bloquea la ventana de terminales.
pub fn bloquear(ventana: &gtk::Window) {
    if esta_bloqueado() || !ajustes::actuales().bloqueo_activado {
        return;
    }
    let Some((capa, caja, _entrada)) = piezas(ventana) else {
        return;
    };
    BLOQUEADO.with(|b| b.set(true));
    DESDE.with(|d| d.set(Instant::now()));
    // Quien se siente delante no debe poder usar las contraseñas de root:
    // fuera de la memoria (al volver se pedirá desbloquear KeePass otra vez).
    boveda::bloquear();

    // Cerrar las demás ventanas de RetroTerm (conexiones, ajustes, "Acerca
    // de"...): ninguna debe quedar a la vista. Se pueden reabrir después.
    if let Some(app) = ventana.application() {
        for otra in app.windows() {
            if &otra != ventana {
                otra.close();
            }
        }
    }

    // Ni teclado ni ratón llegan a las terminales ni al bloc de notas.
    if let Some(contenido) = ventana_terminal::contenido_de(ventana) {
        contenido.set_sensitive(false);
    }
    caja.set_visible(false);
    capa.set_visible(true);
    // Dos "traits" (GtkWindowExt y RootExt) tienen un método `set_focus`:
    // Rust no adivina cuál queremos, así que lo decimos con la ruta completa.
    GtkWindowExt::set_focus(ventana, None::<&gtk::Widget>);
    ventana.set_title(Some("RetroTerm — bloqueado"));
}

/// Quita el bloqueo (tras comprobar la contraseña).
fn desbloquear(ventana: &gtk::Window) {
    let Some((capa, caja, _entrada)) = piezas(ventana) else {
        return;
    };
    BLOQUEADO.with(|b| b.set(false));
    ULTIMA_ACTIVIDAD.with(|u| u.set(Instant::now()));
    caja.set_visible(false);
    capa.set_visible(false);
    if let Some(contenido) = ventana_terminal::contenido_de(ventana) {
        contenido.set_sensitive(true);
    }
    ventana_terminal::tras_desbloquear(ventana);
}

/// Muestra la caja de la contraseña y arranca la cuenta atrás que la
/// ocultará si pasan 10 s sin teclear.
fn mostrar_caja(caja: &gtk::Box, entrada: &gtk::PasswordEntry) {
    caja.set_visible(true);
    entrada.grab_focus();
    ULTIMA_TECLA.with(|u| u.set(Instant::now()));

    // Nueva generación: si quedara un temporizador de antes, se parará solo.
    let generacion = GENERACION_CAJA.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    // Referencias débiles: si la ventana se cierra, el temporizador no la
    // mantiene viva (y se para al no poder «subirlas»).
    let (caja, entrada) = (caja.downgrade(), entrada.downgrade());
    // Cada medio segundo: ¿sigue a la vista? ¿cuánto hace de la última tecla?
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let (Some(caja), Some(entrada)) = (caja.upgrade(), entrada.upgrade()) else {
            return glib::ControlFlow::Break;
        };
        let otra_generacion = GENERACION_CAJA.with(Cell::get) != generacion;
        if otra_generacion || !caja.is_visible() || !esta_bloqueado() {
            return glib::ControlFlow::Break; // ya se ocultó por otro lado
        }
        // Mientras se comprueba la contraseña (casilla desactivada), no
        // se oculta: contamos desde ahora.
        if !entrada.is_sensitive() {
            ULTIMA_TECLA.with(|u| u.set(Instant::now()));
            return glib::ControlFlow::Continue;
        }
        if ULTIMA_TECLA.with(|u| u.get().elapsed()) >= ESPERA_CAJA {
            ocultar_caja(&caja, &entrada);
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
}

/// Oculta la caja de la contraseña y vuelve al salvapantallas: borra lo
/// que se hubiera escrito a medias y el mensaje de estado, y quita el foco
/// (para que las teclas no vayan a una casilla invisible).
fn ocultar_caja(caja: &gtk::Box, entrada: &gtk::PasswordEntry) {
    entrada.set_text("");
    // El mensaje de estado es el hermano siguiente de la casilla.
    // `and_downcast`: si existe Y es una etiqueta, `Some(etiqueta)`.
    if let Some(estado) = entrada.next_sibling().and_downcast::<gtk::Label>() {
        estado.set_text("");
    }
    caja.set_visible(false);
    if let Some(ventana) = caja.root().and_downcast::<gtk::Window>() {
        GtkWindowExt::set_focus(&ventana, None::<&gtk::Widget>);
    }
    // Como al bloquear: durante un segundo el ratón no cuenta (si no, la
    // mano sobre el ratón la volvería a sacar al instante).
    DESDE.with(|d| d.set(Instant::now()));
}

/// Busca en la ventana la capa, la caja y la casilla de la contraseña.
///
/// Devuelve una tupla dentro de un Option: o las tres piezas, o None.
fn piezas(ventana: &gtk::Window) -> Option<(gtk::Overlay, gtk::Box, gtk::PasswordEntry)> {
    let raiz = ventana.child().and_downcast::<gtk::Overlay>()?;
    // Recorremos los hijos de la raíz buscando el que tiene la clase "bloqueo".
    let mut hijo = raiz.first_child();
    let capa = loop {
        let widget = hijo?; // sin más hijos: None
        if widget.has_css_class("bloqueo") {
            break widget.downcast::<gtk::Overlay>().ok()?; // `break valor` sale del loop CON ese valor
        }
        hijo = widget.next_sibling();
    };
    // Dentro de la capa, la caja es el hijo con clase "caja-clave".
    let mut hijo = capa.first_child();
    let caja = loop {
        let widget = hijo?;
        if widget.has_css_class("caja-clave") {
            break widget.downcast::<gtk::Box>().ok()?;
        }
        hijo = widget.next_sibling();
    };
    // Y la casilla es el segundo hijo de la caja (título, casilla, estado).
    let entrada = caja
        .first_child()?
        .next_sibling()?
        .downcast::<gtk::PasswordEntry>()
        .ok()?;
    Some((capa, caja, entrada))
}
