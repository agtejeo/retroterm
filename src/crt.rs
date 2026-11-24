// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Efectos CRT: una capa transparente dibujada ENCIMA de la terminal.
//!
//! La terminal y la capa van dentro de un `gtk::Overlay` (un widget que
//! apila hijos uno sobre otro). La terminal no va sola: va envuelta en un
//! `Halo` (halo.rs), que le añade el resplandor del fósforo:
//!
//!   página: gtk::Overlay
//!   ├─ Halo
//!   │  └─ vte4::Terminal
//!   └─ DrawingArea (esta capa CRT, encima del halo)
//!
//! Para ir de la página a la terminal y al revés, usa `terminal_de()` y
//! `pagina_de()` (así, si la estructura cambia, solo se toca este fichero). La capa es un `DrawingArea`, un lienzo en el
//! que pintamos con Cairo (la librería de dibujo 2D que usa GTK):
//!
//! 1. Líneas de barrido: franjas oscuras horizontales, como las de un tubo.
//! 2. Viñeteado: los bordes se oscurecen poco a poco, como el cristal curvo.
//! 3. Marco: el bisel del monitor, con esquinas redondeadas y un leve brillo
//!    del fósforo en el borde del cristal.
//!
//! La capa no recibe clics (`set_can_target(false)`): el ratón la atraviesa
//! y llega a la terminal, así que seleccionar y copiar siguen funcionando.
//! Solo se vuelve a pintar cuando cambia el tamaño de la ventana, así que
//! apenas consume.

use std::f64::consts::PI;

use gtk::cairo;
use vte4::gtk;
use vte4::prelude::*;
use vte4::Terminal;

use crate::ajustes;
use crate::halo::Halo;

/// Cada cuántos píxeles hay una línea de barrido (de 1 píxel de alto).
const SEPARACION_LINEAS: f64 = 3.0;
// La oscuridad de las líneas ya no es una constante: es un ajuste
// (Ajustes → "Intensidad de las líneas"), ver `crt_intensidad`.
/// Oscuridad máxima de los bordes por el viñeteado.
const OPACIDAD_VINETA: f64 = 0.45;
/// Grosor del marco (bisel) del monitor, en píxeles.
const GROSOR_MARCO: f64 = 7.0;
/// Radio de las esquinas de la "pantalla" de cristal.
const RADIO_ESQUINAS: f64 = 26.0;
/// Color del plástico del marco (gris muy oscuro), en RGB de 0.0 a 1.0.
const COLOR_MARCO: (f64, f64, f64) = (0.14, 0.14, 0.13);

/// Mete la terminal en un Overlay con la capa CRT encima.
///
/// Devuelve el Overlay: es lo que se coloca como página de la pestaña.
/// La capa siempre está; qué efectos pinta lo deciden los ajustes actuales.
pub fn envolver(terminal: &Terminal) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&Halo::new(terminal)));

    let capa = gtk::DrawingArea::new();
    // Ni el ratón ni el teclado se quedan en la capa: la atraviesan.
    capa.set_can_target(false);
    capa.set_can_focus(false);
    // GTK llama a esta función cada vez que hay que pintar la capa.
    capa.set_draw_func(|_capa, cr, ancho, alto| {
        if let Err(error) = dibujar(cr, ancho as f64, alto as f64) {
            eprintln!("Error al dibujar el efecto CRT: {error}");
        }
    });
    overlay.add_overlay(&capa);
    overlay
}

/// Pide a GTK que vuelva a pintar la capa CRT de una página (tras cambiar
/// los ajustes).
pub fn repintar(overlay: &gtk::Overlay) {
    // Recorremos los hijos del Overlay como una lista enlazada:
    // el primer hijo y, de cada uno, su "hermano" siguiente.
    let mut hijo = overlay.first_child();
    while let Some(widget) = hijo {
        if let Some(capa) = widget.downcast_ref::<gtk::DrawingArea>() {
            capa.queue_draw();
        } else if let Some(halo) = widget.downcast_ref::<Halo>() {
            halo.queue_draw(); // por si cambió el halo en Ajustes
        }
        hijo = widget.next_sibling();
    }
}

/// La terminal de una página (página → Halo → Terminal).
///
/// `?` en una función que devuelve Option: si un paso da None, se sale ya
/// devolviendo None.
pub fn terminal_de(pagina: &gtk::Widget) -> Option<Terminal> {
    let overlay = pagina.downcast_ref::<gtk::Overlay>()?;
    let halo = overlay.child().and_downcast::<Halo>()?;
    halo.hijo().and_downcast::<Terminal>()
}

/// La página (el Overlay de `envolver`) en la que está una terminal: el
/// primer Overlay que hay subiendo desde ella por el árbol de widgets.
pub fn pagina_de(terminal: &Terminal) -> Option<gtk::Widget> {
    terminal.ancestor(gtk::Overlay::static_type())
}

/// Pinta los tres efectos, uno encima de otro.
///
/// Casi todas las operaciones de Cairo devuelven `Result`, así que usamos `?`
/// en cada una: al primer error, se para y lo devuelve.
fn dibujar(cr: &cairo::Context, ancho: f64, alto: f64) -> Result<(), cairo::Error> {
    let ajustes = ajustes::actuales();
    if ajustes.crt_lineas {
        lineas_de_barrido(cr, ancho, alto, ajustes.crt_intensidad)?;
    }
    if ajustes.crt_vineta {
        vineteado(cr, ancho, alto)?;
    }
    if ajustes.crt_esquinas {
        // El brillo del borde, del color del fósforo actual.
        let fosforo = gtk::gdk::RGBA::parse(ajustes.tema().texto)
            .unwrap_or(gtk::gdk::RGBA::new(0.2, 1.0, 0.4, 1.0));
        marco(cr, ancho, alto, &fosforo)?;
    }
    Ok(())
}

fn lineas_de_barrido(
    cr: &cairo::Context,
    ancho: f64,
    alto: f64,
    intensidad: f64,
) -> Result<(), cairo::Error> {
    cr.set_source_rgba(0.0, 0.0, 0.0, intensidad);
    // Primero añadimos TODOS los rectángulos al "trazado" y al final los
    // rellenamos de una vez: mucho más rápido que rellenar uno a uno.
    // `let mut y` = variable que vamos a cambiar; `while` repite mientras se cumpla.
    let mut y = 0.0;
    while y < alto {
        cr.rectangle(0.0, y, ancho, 1.0);
        y += SEPARACION_LINEAS;
    }
    cr.fill()
}

fn vineteado(cr: &cairo::Context, ancho: f64, alto: f64) -> Result<(), cairo::Error> {
    let (centro_x, centro_y) = (ancho / 2.0, alto / 2.0);
    // Distancia del centro a una esquina (Pitágoras).
    let radio = (centro_x * centro_x + centro_y * centro_y).sqrt();

    // Degradado circular: transparente en el centro, oscuro en los bordes.
    // Empieza a oscurecer a partir del 55 % del radio.
    let degradado =
        cairo::RadialGradient::new(centro_x, centro_y, radio * 0.55, centro_x, centro_y, radio);
    degradado.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, 0.0);
    degradado.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, OPACIDAD_VINETA);
    cr.set_source(&degradado)?;
    cr.paint() // `paint` = rellenar todo el lienzo con la fuente actual
}

/// El bisel del monitor: todo lo que queda FUERA de un rectángulo
/// redondeado se pinta de gris oscuro; y el borde del cristal, con un
/// brillo tenue del color del fósforo.
fn marco(
    cr: &cairo::Context,
    ancho: f64,
    alto: f64,
    fosforo: &gtk::gdk::RGBA,
) -> Result<(), cairo::Error> {
    let g = GROSOR_MARCO;
    let (x, y, w, h) = (g, g, ancho - 2.0 * g, alto - 2.0 * g);

    // Truco: trazamos el rectángulo completo Y el redondeado de dentro. Con
    // la regla "par-impar" se rellena solo lo que queda entre los dos.
    cr.set_fill_rule(cairo::FillRule::EvenOdd);
    cr.rectangle(0.0, 0.0, ancho, alto);
    rectangulo_redondeado(cr, x, y, w, h, RADIO_ESQUINAS);
    let (r, v, a) = COLOR_MARCO; // desestructurar una tupla en tres variables
    cr.set_source_rgb(r, v, a);
    cr.fill()?;

    // Brillo del borde del cristal: una línea fina y semitransparente.
    rectangulo_redondeado(cr, x, y, w, h, RADIO_ESQUINAS);
    cr.set_source_rgba(
        fosforo.red() as f64, // RGBA guarda f32; Cairo quiere f64: `as` convierte
        fosforo.green() as f64,
        fosforo.blue() as f64,
        0.35,
    );
    cr.set_line_width(1.5);
    cr.stroke()
}

/// Añade al trazado un rectángulo con las esquinas redondeadas.
fn rectangulo_redondeado(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    // Cada `arc` es un cuarto de círculo: (centro x, centro y, radio, ángulo inicial, final).
    // Los ángulos van en radianes: PI = media vuelta.
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0); // arriba a la derecha
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0); // abajo a la derecha
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI); // abajo a la izquierda
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0); // arriba a la izquierda
    cr.close_path();
}
