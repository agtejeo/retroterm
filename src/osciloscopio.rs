// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! El salvapantallas "osciloscopio": curvas de Lissajous con la persistencia
//! del fósforo, como en un monitor vectorial.
//!
//! Una curva de Lissajous es lo que dibuja un punto cuya x y cuya y oscilan
//! con frecuencias distintas:  x = sen(a·s + fase),  y = sen(b·s).
//! Con a:b = 3:2, 5:4... salen figuras distintas; moviendo la fase, giran.
//!
//! La estela (persistencia) la da el `Lienzo` de salvapantallas.rs.

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{cairo, glib};
use vte4::gtk;

use crate::ajustes;
use crate::salvapantallas::{self, Lienzo};

/// Las figuras (frecuencias a:b) por las que va pasando.
const FIGURAS: [(f64, f64); 5] = [(3.0, 2.0), (5.0, 4.0), (3.0, 4.0), (1.0, 2.0), (5.0, 6.0)];
/// Segundos que dura cada figura.
const SEGUNDOS_POR_FIGURA: f64 = 20.0;
/// Cuánto se apaga la estela en cada paso (más alto = estela más corta).
const DESVANECIMIENTO: f64 = 0.10;
/// Pasos por segundo de la animación (unos 30: suave y sin gastar de más).
const PASO: Duration = Duration::from_millis(33);
/// Grosor de la línea de la curva, en píxeles.
const GROSOR: f64 = 2.0;
/// Margen (en píxeles) entre la curva y el borde de la pantalla: lo justo
/// para que la línea no se corte a medias al tocar el borde.
const MARGEN: f64 = GROSOR + 2.0;

struct Estado {
    lienzo: Lienzo,
    inicio: Instant,
    ultimo_paso: Instant,
}

/// Crea el área de dibujo del osciloscopio.
pub fn nuevo() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let estado = Rc::new(RefCell::new(Estado {
        lienzo: Lienzo::nuevo(),
        inicio: Instant::now(),
        ultimo_paso: Instant::now(),
    }));

    area.set_draw_func(move |_area, cr, ancho, alto| {
        let mut estado = estado.borrow_mut();
        if let Err(error) = dibujar(&mut estado, cr, ancho, alto) {
            eprintln!("Error al dibujar el osciloscopio: {error}");
        }
    });

    // "Tick callback": GTK nos llama en CADA fotograma de la pantalla mientras
    // el área esté visible. Pedimos repintar; si está oculta, no se llama.
    area.add_tick_callback(|area, _reloj| {
        area.queue_draw();
        glib::ControlFlow::Continue
    });
    area
}

fn dibujar(
    estado: &mut Estado,
    cr: &cairo::Context,
    ancho: i32,
    alto: i32,
) -> Result<(), cairo::Error> {
    let tema = ajustes::actuales().tema();
    let fondo = salvapantallas::color(tema.fondo);
    let texto = salvapantallas::color(tema.texto);

    estado.lienzo.preparar(ancho, alto, fondo)?;

    // Avanzar la animación (como mucho, un paso cada PASO).
    if estado.ultimo_paso.elapsed() >= PASO {
        estado.ultimo_paso = Instant::now();
        let t = estado.inicio.elapsed().as_secs_f64();
        // El closure `|c| ...` es "cómo se dibuja este paso"; el lienzo se
        // encarga de oscurecer lo anterior y de limpiar restos.
        estado
            .lienzo
            .paso(DESVANECIMIENTO, |c| pintar_curva(c, t, ancho as f64, alto as f64, texto))?;
    }

    estado.lienzo.mostrar(cr)?;
    salvapantallas::pintar_encima(cr, ancho, alto)
}

/// La curva de ahora, pintada sobre el lienzo persistente.
fn pintar_curva(
    c: &cairo::Context,
    t: f64,
    ancho: f64,
    alto: f64,
    texto: (f64, f64, f64),
) -> Result<(), cairo::Error> {
    // `%` = resto de la división: vuelve a empezar la lista.
    let figura = (t / SEGUNDOS_POR_FIGURA) as usize % FIGURAS.len();
    let (a, b) = FIGURAS[figura];
    let fase = t * 0.6;
    // La curva ocupa toda la pantalla: radio = mitad del lado menos el margen.
    // Se calcula en cada paso con el tamaño actual, así que se adapta sola
    // al reescalar la ventana. `.max(0.0)`: por si la ventana es diminuta.
    let (centro_x, centro_y) = (ancho / 2.0, alto / 2.0);
    let radio_x = (ancho / 2.0 - MARGEN).max(0.0);
    let radio_y = (alto / 2.0 - MARGEN).max(0.0);

    c.set_source_rgb(texto.0, texto.1, texto.2);
    c.set_line_width(GROSOR);
    const PUNTOS: usize = 800;
    for i in 0..=PUNTOS {
        let s = i as f64 / PUNTOS as f64 * 2.0 * PI;
        let x = centro_x + radio_x * (a * s + fase).sin();
        let y = centro_y + radio_y * (b * s).sin();
        if i == 0 {
            c.move_to(x, y);
        } else {
            c.line_to(x, y);
        }
    }
    c.stroke()
}
