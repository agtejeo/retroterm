// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! El salvapantallas "lluvia de letras": columnas de caracteres que caen,
//! con la cabeza casi blanca y una estela de fósforo que se va apagando.
//!
//! La pantalla se divide en una rejilla de celdas del tamaño de una letra.
//! Cada columna tiene una "gota" (su cabeza) que baja a su propia velocidad.
//! Cada vez que la cabeza entra en una fila nueva:
//!   - la celda que deja atrás se repinta con el color normal del fósforo;
//!   - en la nueva se pinta una letra al azar, más brillante.
//! La estela no se dibuja: es el `Lienzo` (salvapantallas.rs) que va
//! oscureciendo poco a poco lo pintado antes.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{cairo, glib};
use vte4::gtk;

use crate::ajustes;
use crate::salvapantallas::{self, Lienzo};

/// Las letras que caen. Todas ASCII: así seguro que están en la fuente retro
/// (VT323 y similares no traen otros alfabetos).
const CARACTERES: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ@#$%&*+=<>?:;";
/// Cuánto se apaga la estela en cada paso (más bajo = estela más larga).
const DESVANECIMIENTO: f64 = 0.06;
/// Pasos por segundo de la animación (unos 30).
const PASO: Duration = Duration::from_millis(33);
/// Velocidad de las gotas, en filas por paso (cada una, al azar entre ambas).
const VELOCIDAD_MIN: f64 = 0.25;
const VELOCIDAD_MAX: f64 = 0.9;
/// Cuánto se acerca al blanco el color de la cabeza (0 = fósforo, 1 = blanco).
const BRILLO_CABEZA: f64 = 0.7;

/// La cabeza de una columna que cae.
struct Gota {
    /// Fila en la que está (con decimales: avanza un poco en cada paso).
    fila: f64,
    /// Filas que baja en cada paso.
    velocidad: f64,
    /// Última fila en la que se pintó la cabeza.
    ultima: i32,
    /// La letra que hay en esa fila (para repintarla al dejarla atrás).
    caracter: char,
}

struct Estado {
    lienzo: Lienzo,
    /// Una gota por columna. `Vec` = lista que puede crecer o encoger.
    gotas: Vec<Gota>,
    /// Tamaño de cada celda (ancho, alto) en píxeles.
    celda: (f64, f64),
    ultimo_paso: Instant,
}

/// Crea el área de dibujo de la lluvia.
pub fn nuevo() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let estado = Rc::new(RefCell::new(Estado {
        lienzo: Lienzo::nuevo(),
        gotas: Vec::new(),
        celda: (0.0, 0.0),
        ultimo_paso: Instant::now(),
    }));

    area.set_draw_func(move |_area, cr, ancho, alto| {
        let mut estado = estado.borrow_mut();
        if let Err(error) = dibujar(&mut estado, cr, ancho, alto) {
            eprintln!("Error al dibujar la lluvia de letras: {error}");
        }
    });
    // Repintar en cada fotograma mientras esté visible (ver osciloscopio.rs).
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
    let ajustes = ajustes::actuales();
    let tema = ajustes.tema();
    let fondo = salvapantallas::color(tema.fondo);
    let texto = salvapantallas::color(tema.texto);
    let cabeza = mezclar(texto, (1.0, 1.0, 1.0), BRILLO_CABEZA);
    let tamano_letra = ajustes.tamano as f64;

    // Medimos la celda con la fuente de los ajustes: el ancho que avanza
    // una "M" (en una fuente monoespaciada, todas avanzan lo mismo).
    poner_fuente(cr, &ajustes.fuente, tamano_letra);
    let celda = (cr.text_extents("M")?.x_advance().max(1.0), tamano_letra);

    // Si el lienzo es nuevo (tamaño o tema distintos), o cambió la fuente,
    // repartimos las gotas de nuevo: una por columna.
    let recreado = estado.lienzo.preparar(ancho, alto, fondo)?;
    if recreado || celda != estado.celda || estado.gotas.is_empty() {
        estado.celda = celda;
        let columnas = ((ancho as f64 / celda.0) as usize).max(1);
        let filas = alto as f64 / celda.1;
        // `(0..n).map(...).collect()`: crea una lista de n elementos.
        estado.gotas = (0..columnas).map(|_| nueva_gota(filas, true)).collect();
    }

    if estado.ultimo_paso.elapsed() >= PASO {
        estado.ultimo_paso = Instant::now();
        let filas = alto as f64 / celda.1;
        let fuente = ajustes.fuente.clone();
        // Préstamo mutable de `gotas` y, a la vez, de `lienzo`: se puede
        // porque son CAMPOS distintos de `estado` (Rust lo ve).
        let gotas = &mut estado.gotas;
        estado.lienzo.paso(DESVANECIMIENTO, |c| {
            poner_fuente(c, &fuente, tamano_letra);
            // `iter_mut()` da un `&mut Gota` de cada una; `enumerate()`
            // añade su posición (la columna).
            for (columna, gota) in gotas.iter_mut().enumerate() {
                let x = columna as f64 * celda.0;
                gota.fila += gota.velocidad;
                // `floor()` redondea hacia abajo; `as i32` lo pasa a entero.
                let fila_actual = gota.fila.floor() as i32;
                // Puede haber avanzado más de una fila: las recorremos todas.
                while gota.ultima < fila_actual {
                    pintar_letra(c, x, gota.ultima, celda, gota.caracter, texto, fondo)?;
                    gota.ultima += 1;
                    gota.caracter = letra_al_azar();
                    pintar_letra(c, x, gota.ultima, celda, gota.caracter, cabeza, fondo)?;
                }
                // Salió por abajo: otra gota en esa columna, desde arriba.
                if gota.fila > filas + 1.0 {
                    // `*gota = ...` sustituye la gota entera (a través del `&mut`).
                    *gota = nueva_gota(filas, false);
                }
            }
            Ok(())
        })?;
    }

    estado.lienzo.mostrar(cr)?;
    salvapantallas::pintar_encima(cr, ancho, alto)
}

/// Una gota nueva por encima de la pantalla (fila negativa), para que no
/// entren todas a la vez. Al principio se reparten más (`al_principio`).
fn nueva_gota(filas: f64, al_principio: bool) -> Gota {
    let altura_max = if al_principio { filas } else { filas * 0.5 };
    // `glib::random_double_range(a, b)`: número al azar entre a y b.
    // (Usamos el de GLib para no añadir otra dependencia.)
    let fila = -glib::random_double_range(0.0, altura_max.max(1.0));
    Gota {
        fila,
        velocidad: glib::random_double_range(VELOCIDAD_MIN, VELOCIDAD_MAX),
        ultima: fila.floor() as i32,
        caracter: letra_al_azar(),
    }
}

fn letra_al_azar() -> char {
    // `as_bytes()`: como todas son ASCII, cada letra es exactamente 1 byte.
    let letras = CARACTERES.as_bytes();
    let i = glib::random_int_range(0, letras.len() as i32) as usize;
    letras[i] as char
}

/// Pinta una letra en la celda (columna en `x`, fila `fila`): primero borra
/// la celda con el fondo (para que no se monten dos letras) y luego la letra.
fn pintar_letra(
    c: &cairo::Context,
    x: f64,
    fila: i32,
    celda: (f64, f64),
    letra: char,
    color: (f64, f64, f64),
    fondo: (f64, f64, f64),
) -> Result<(), cairo::Error> {
    if fila < 0 {
        return Ok(()); // aún por encima de la pantalla
    }
    let y = fila as f64 * celda.1;
    c.set_source_rgb(fondo.0, fondo.1, fondo.2);
    c.rectangle(x, y, celda.0, celda.1);
    c.fill()?;
    c.set_source_rgb(color.0, color.1, color.2);
    // La "línea base" del texto queda a un 80 % de la celda (lo de abajo es
    // para las letras con rabito, como la "g").
    c.move_to(x, y + celda.1 * 0.8);
    // `encode_utf8` escribe la letra en un pequeño búfer y devuelve un &str
    // prestado de él: así no creamos un String en cada letra.
    let mut bufer = [0u8; 4];
    c.show_text(letra.encode_utf8(&mut bufer))
}

fn poner_fuente(c: &cairo::Context, fuente: &str, tamano: f64) {
    c.select_font_face(fuente, cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    c.set_font_size(tamano);
}

/// Mezcla dos colores: `t` = 0.0 da `a`, 1.0 da `b`.
fn mezclar(a: (f64, f64, f64), b: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}
