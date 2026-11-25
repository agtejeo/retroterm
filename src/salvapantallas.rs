// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Lo común a los salvapantallas de la pantalla de bloqueo:
//!
//! - `nuevo()`: el widget que va en la capa de bloqueo. Es un `gtk::Stack`
//!   (una "pila" que muestra solo uno de sus hijos) con un hijo por efecto;
//!   al aparecer, enseña el efecto elegido en Ajustes.
//! - `Lienzo`: la superficie persistente que da la estela del fósforo.
//! - `pintar_encima()`: líneas de barrido y el rótulo con la hora.
//!
//! La PERSISTENCIA: el lienzo NO se borra nunca. En cada paso se le pinta
//! encima una capa casi transparente del color de fondo (lo de antes se
//! oscurece un poco) y después lo nuevo, brillante.
//!
//! Problema de esa técnica: el lienzo guarda cada color con 8 bits (0-255).
//! Si un píxel está a 5 niveles del fondo, oscurecerlo un 10 % daría 4,5,
//! que se REDONDEA otra vez a 5: el píxel no llega nunca al fondo y quedan
//! restos tenues que van llenando la pantalla. Por eso, además, hay una
//! "limpieza" que acerca cada píxel al fondo de 1 en 1 (`limpiar_franja`).

use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use vte4::gtk;

use crate::ajustes::{self, Salvapantallas};
use crate::invasion;
use crate::lluvia;
use crate::osciloscopio;
use crate::sobrevuelo;

/// La limpieza de restos se reparte en este número de franjas horizontales:
/// en cada paso se limpia una sola, para que el coste por fotograma sea
/// pequeño. Cada píxel se limpia unas 30 / 8 ≈ 4 veces por segundo.
const FRANJAS: usize = 8;

/// Crea el widget del salvapantallas para la capa de bloqueo.
pub fn nuevo() -> gtk::Stack {
    let pila = gtk::Stack::new();
    // Cada hijo con un nombre, para poder elegirlo luego por su nombre.
    pila.add_named(&osciloscopio::nuevo(), Some(Salvapantallas::Osciloscopio.id()));
    pila.add_named(&lluvia::nuevo(), Some(Salvapantallas::Lluvia.id()));
    pila.add_named(&invasion::nuevo(), Some(Salvapantallas::Invasion.id()));
    pila.add_named(&sobrevuelo::nuevo(), Some(Salvapantallas::Sobrevuelo.id()));

    // "map" se emite cada vez que el widget aparece en pantalla, o sea, cada
    // vez que se bloquea. Ese es el momento de mirar qué efecto toca: así un
    // cambio en Ajustes vale para el siguiente bloqueo.
    // El hijo que no se ve está oculto y su animación no gasta nada (GTK no
    // llama a su "tick callback").
    pila.connect_map(|pila| {
        let elegido = ajustes::actuales().salvapantallas;
        pila.set_visible_child_name(elegido.id());
    });
    pila
}

/// El lienzo persistente que conserva la estela.
pub struct Lienzo {
    /// La superficie de dibujo. `None` hasta el primer `preparar()`.
    superficie: Option<cairo::ImageSurface>,
    /// El color de fondo tal como quedó guardado en los bytes del lienzo
    /// (leído del propio lienzo para que coincida exactamente con el
    /// redondeo que hace Cairo).
    fondo_bytes: [u8; 4],
    /// El color de fondo con el que se creó (si cambia el tema, se rehace).
    fondo: (f64, f64, f64),
    /// Qué franja toca limpiar en el próximo paso.
    franja: usize,
}

impl Lienzo {
    pub fn nuevo() -> Self {
        Lienzo {
            superficie: None,
            fondo_bytes: [0; 4],
            fondo: (0.0, 0.0, 0.0),
            franja: 0,
        }
    }

    /// Crea (o rehace) la superficie si hace falta: la primera vez, si
    /// cambió el tamaño o si cambió el color de fondo. Devuelve `true` si la
    /// ha creado de nuevo (el efecto puede querer reiniciarse entonces).
    pub fn preparar(
        &mut self,
        ancho: i32,
        alto: i32,
        fondo: (f64, f64, f64),
    ) -> Result<bool, cairo::Error> {
        let hay_que_crear = match &self.superficie {
            Some(superficie) => {
                superficie.width() != ancho || superficie.height() != alto || self.fondo != fondo
            }
            None => true,
        };
        if !hay_que_crear {
            return Ok(false);
        }
        // `mut`: más abajo leemos sus bytes con `data()`, que pide `&mut`.
        let mut superficie = cairo::ImageSurface::create(cairo::Format::Rgb24, ancho, alto)?;
        {
            // Un bloque { } para que este Context se suelte al terminar.
            // Mientras exista, la superficie tiene otro "dueño" y `data()` fallaría.
            let c = cairo::Context::new(&superficie)?;
            c.set_source_rgb(fondo.0, fondo.1, fondo.2);
            c.paint()?;
        }
        // Leemos cómo quedó guardado el fondo (los 4 bytes del primer píxel).
        // `data()` devuelve un Result: con `if let Ok(...)` solo seguimos si
        // salió bien.
        if let Ok(datos) = superficie.data() {
            if datos.len() >= 4 {
                // `copy_from_slice`: copia un trozo de bytes en el array.
                self.fondo_bytes.copy_from_slice(&datos[0..4]);
            }
        } // aquí se suelta `datos` y la superficie vuelve a estar libre
        self.superficie = Some(superficie);
        self.fondo = fondo;
        self.franja = 0;
        Ok(true)
    }

    /// Un paso de la animación: oscurece lo que había (`alfa` = cuánto) y
    /// llama a `pintar` para que dibuje lo nuevo.
    ///
    /// `pintar: impl FnOnce(&cairo::Context) -> Result<...>` = recibimos un
    /// CLOSURE: cada efecto nos pasa "su manera de dibujar" y nosotros nos
    /// ocupamos de lo común (oscurecer, limpiar restos).
    pub fn paso(
        &mut self,
        alfa: f64,
        pintar: impl FnOnce(&cairo::Context) -> Result<(), cairo::Error>,
    ) -> Result<(), cairo::Error> {
        // `let ... else`: si aún no hay superficie, no hay nada que hacer.
        let Some(superficie) = &mut self.superficie else {
            return Ok(());
        };
        {
            // `&*superficie`: de `&mut ImageSurface` a `&ImageSurface`
            // (Context::new solo necesita mirarla, no cambiarla).
            let c = cairo::Context::new(&*superficie)?;
            // 1. Apagar un poco lo que ya había (la estela).
            c.set_source_rgba(self.fondo.0, self.fondo.1, self.fondo.2, alfa);
            c.paint()?;
            // 2. Lo nuevo, lo pinta el efecto. El `?` devuelve su error, si hay.
            pintar(&c)?;
        } // se suelta `c`: la superficie queda libre para `limpiar_franja`
        limpiar_franja(superficie, self.franja, self.fondo_bytes);
        self.franja = (self.franja + 1) % FRANJAS;
        Ok(())
    }

    /// Copia el lienzo a la pantalla.
    pub fn mostrar(&self, cr: &cairo::Context) -> Result<(), cairo::Error> {
        if let Some(superficie) = &self.superficie {
            cr.set_source_surface(superficie, 0.0, 0.0)?;
            cr.paint()?;
            // Cambiamos la "fuente de color" para que `cr` suelte su
            // referencia a la superficie: si no, en el próximo paso
            // `data()` vería que alguien más la usa y no podría limpiar.
            cr.set_source_rgb(0.0, 0.0, 0.0);
        }
        Ok(())
    }
}

/// Acerca al fondo, de 1 en 1, los bytes de una franja de la superficie.
///
/// El oscurecido con Cairo se queda atascado a pocos niveles del fondo por
/// el redondeo (ver arriba). Restar 1 nunca se "redondea a nada", así que
/// esos restos acaban desapareciendo del todo en poco más de un segundo.
fn limpiar_franja(superficie: &mut cairo::ImageSurface, franja: usize, fondo: [u8; 4]) {
    let alto = superficie.height().max(0) as usize;
    // `stride`: bytes que ocupa cada fila en memoria (puede haber relleno).
    let stride = superficie.stride().max(0) as usize;
    // Filas de esta franja: de `desde` a `hasta` (sin incluir).
    let desde = alto * franja / FRANJAS;
    let hasta = alto * (franja + 1) / FRANJAS;

    // `data()` da acceso directo a los bytes (falla si alguien más está
    // usando la superficie; entonces, simplemente, no limpiamos).
    // Al soltar `datos` (fin de la función) Cairo se entera de que cambiaron.
    let Ok(mut datos) = superficie.data() else {
        return;
    };
    let fin = (hasta * stride).min(datos.len());
    let trozo = &mut datos[desde * stride..fin];

    // Formato Rgb24: 4 bytes por píxel. `chunks_exact_mut(4)` recorre el
    // trozo de 4 en 4 dando un `&mut [u8]` (préstamo mutable) de cada píxel.
    for pixel in trozo.chunks_exact_mut(4) {
        // `iter_mut().zip(...)`: recorre a la vez los 4 bytes del píxel y
        // los 4 del fondo, por parejas. `*byte` = el valor al que apunta.
        for (byte, objetivo) in pixel.iter_mut().zip(fondo) {
            if *byte > objetivo {
                *byte -= 1;
            } else if *byte < objetivo {
                *byte += 1;
            }
        }
    }
}

/// Lo que va encima de cualquier efecto: las líneas de barrido (si están
/// activadas en Ajustes) y el rótulo con la hora, abajo en el centro.
pub fn pintar_encima(cr: &cairo::Context, ancho: i32, alto: i32) -> Result<(), cairo::Error> {
    let ajustes = ajustes::actuales();
    let tema = ajustes.tema();
    let fondo = color(tema.fondo);
    let texto = color(tema.texto);

    if ajustes.crt_lineas {
        cr.set_source_rgba(0.0, 0.0, 0.0, ajustes.crt_intensidad);
        let mut y = 0.0;
        while y < alto as f64 {
            cr.rectangle(0.0, y, ancho as f64, 1.0);
            y += 3.0;
        }
        cr.fill()?;
    }

    let hora = glib::DateTime::now_local()
        .and_then(|ahora| ahora.format("%H:%M"))
        .map(|hora| hora.to_string())
        .unwrap_or_default();
    let rotulo = format!("▒ TERMINAL BLOQUEADO ▒   {hora}");
    cr.select_font_face(
        &ajustes.fuente,
        cairo::FontSlant::Normal,
        cairo::FontWeight::Normal,
    );
    cr.set_font_size(22.0);
    let medidas = cr.text_extents(&rotulo)?;
    let x = (ancho as f64 - medidas.width()) / 2.0;
    let y = alto as f64 - 30.0;
    // El efecto pasa por detrás del rótulo: le ponemos un recuadro del
    // color de fondo para que se lea bien.
    // `y_bearing` es negativo: lo que sube el texto desde la línea base.
    cr.set_source_rgb(fondo.0, fondo.1, fondo.2);
    cr.rectangle(
        x - 10.0,
        y + medidas.y_bearing() - 6.0,
        medidas.width() + 20.0,
        medidas.height() + 12.0,
    );
    cr.fill()?;
    cr.set_source_rgba(texto.0, texto.1, texto.2, 0.8);
    cr.move_to(x, y);
    cr.show_text(&rotulo)?;
    Ok(())
}

/// "#33ff66" -> (r, g, b) de 0.0 a 1.0, como lo quiere Cairo.
pub fn color(hex: &str) -> (f64, f64, f64) {
    let rgba = gdk::RGBA::parse(hex).unwrap_or(gdk::RGBA::new(0.2, 1.0, 0.4, 1.0));
    (rgba.red() as f64, rgba.green() as f64, rgba.blue() as f64)
}
