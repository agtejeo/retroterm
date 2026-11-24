// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Paleta monocroma: los 256 colores de terminal convertidos a tonos del fósforo.
//!
//! Los programas no escriben "verde #33ff66": piden "color 4" (azul) o
//! "color 196" (un rojo). La terminal traduce cada número a un color real
//! mediante su PALETA. Si la paleta solo contiene tonos de fósforo, todo
//! sale monocromo, como en un monitor de los 80: lo que era un color oscuro
//! (azul) sale como fósforo tenue y lo que era claro (blanco), como fósforo
//! brillante.
//!
//! El método:
//! 1. Tomamos el color estándar de xterm para cada número (0..=255).
//! 2. Calculamos su luminancia: cuánta luz percibe el ojo (el verde cuenta
//!    mucho más que el azul).
//! 3. La convertimos en un "nivel" de fósforo y en un color entre el fondo
//!    y el texto del tema; los más brillantes se acercan al blanco.
//!
//! Limitación: los programas que usan "color verdadero" (24 bits, p. ej.
//! algunos temas de vim) indican el RGB directamente y no pasan por la paleta.

use vte4::gtk::gdk;

/// Los 16 colores básicos de xterm como (rojo, verde, azul) de 0 a 255.
/// `[T; 16]` = array de tamaño fijo: 16 elementos, ni uno más ni uno menos.
const BASICOS: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00), // 0  negro
    (0xcd, 0x00, 0x00), // 1  rojo
    (0x00, 0xcd, 0x00), // 2  verde
    (0xcd, 0xcd, 0x00), // 3  amarillo
    (0x00, 0x00, 0xee), // 4  azul
    (0xcd, 0x00, 0xcd), // 5  magenta
    (0x00, 0xcd, 0xcd), // 6  cian
    (0xe5, 0xe5, 0xe5), // 7  blanco
    (0x7f, 0x7f, 0x7f), // 8  gris (negro brillante)
    (0xff, 0x00, 0x00), // 9  rojo brillante
    (0x00, 0xff, 0x00), // 10 verde brillante
    (0xff, 0xff, 0x00), // 11 amarillo brillante
    (0x5c, 0x5c, 0xff), // 12 azul brillante
    (0xff, 0x00, 0xff), // 13 magenta brillante
    (0x00, 0xff, 0xff), // 14 cian brillante
    (0xff, 0xff, 0xff), // 15 blanco brillante
];

/// Nivel del negro: algo por encima del fondo, para que el texto escrito
/// "en negro" no desaparezca del todo.
const NIVEL_MINIMO: f32 = 0.35;
/// Nivel del blanco brillante: por encima de 1.0 el fósforo tira a blanco.
const NIVEL_MAXIMO: f32 = 1.30;

/// La paleta completa de 256 colores para `Terminal::set_colors`.
pub fn monocroma(fondo: &gdk::RGBA, texto: &gdk::RGBA) -> Vec<gdk::RGBA> {
    // Un rango (0..256) también es un iterador: lo transformamos paso a paso.
    (0..256)
        .map(color_xterm)
        .map(|rgb| a_fosforo(nivel(rgb), fondo, texto))
        .collect()
}

/// El color estándar de xterm para el número `i`.
fn color_xterm(i: usize) -> (u8, u8, u8) {
    // `match` con RANGOS: `0..=15` incluye el 15.
    match i {
        0..=15 => BASICOS[i],
        16..=231 => {
            // Un cubo de 6×6×6 colores: cada componente tiene 6 escalones.
            let n = i - 16;
            let escalon = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            (escalon(n / 36), escalon((n / 6) % 6), escalon(n % 6))
        }
        // `_` = "cualquier otro caso": aquí, 232..=255, una escala de grises.
        _ => {
            let gris = (8 + (i - 232) * 10) as u8;
            (gris, gris, gris)
        }
    }
}

/// Cuánta luz percibimos de un color, de 0.0 (negro) a 1.0 (blanco).
///
/// El parámetro se "desestructura" en la propia firma: recibimos una tupla
/// y la abrimos directamente en `(r, g, b)`.
fn luminancia((r, g, b): (u8, u8, u8)) -> f32 {
    // Los valores de pantalla no son lineales: los pasamos a "luz real" (^2.2).
    let lineal = |c: u8| (c as f32 / 255.0).powf(2.2);
    0.2126 * lineal(r) + 0.7152 * lineal(g) + 0.0722 * lineal(b)
}

/// Luminancia -> nivel de fósforo. La raíz cuadrada aclara los tonos oscuros
/// (sin ella, el azul de los directorios de `ls` apenas se vería).
fn nivel(rgb: (u8, u8, u8)) -> f32 {
    NIVEL_MINIMO + luminancia(rgb).sqrt() * (NIVEL_MAXIMO - NIVEL_MINIMO)
}

/// Nivel -> color: de 0 a 1, del fondo al texto; por encima de 1, del texto al blanco.
fn a_fosforo(nivel: f32, fondo: &gdk::RGBA, texto: &gdk::RGBA) -> gdk::RGBA {
    if nivel <= 1.0 {
        mezclar(fondo, texto, nivel)
    } else {
        let blanco = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
        mezclar(texto, &blanco, nivel - 1.0)
    }
}

/// Mezcla dos colores: t = 0.0 da `a`, t = 1.0 da `b`.
fn mezclar(a: &gdk::RGBA, b: &gdk::RGBA, t: f32) -> gdk::RGBA {
    let entre = |x: f32, y: f32| x + (y - x) * t;
    gdk::RGBA::new(
        entre(a.red(), b.red()),
        entre(a.green(), b.green()),
        entre(a.blue(), b.blue()),
        1.0,
    )
}
