// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! "Espera este texto → escribe aquello": la pieza que teclea por ti.
//!
//! Vigilamos la línea donde está el cursor. Cuando EMPIEZA o TERMINA por el
//! texto esperado, escribimos la respuesta y pulsamos Intro. Ejemplos:
//!   "usuario@web19's password:"       termina por "password:"
//!   "usuario@web19:~$"                termina por "$"
//!   "[sudo] password for usuario:"    empieza por "[sudo]"
//!
//! Reglas de seguridad:
//! - Los pasos se cumplen en orden y cada uno UNA sola vez.
//! - Cada paso tiene un plazo (contado desde el paso anterior). Si un paso
//!   obligatorio no llega a tiempo, se abandona el resto: así nunca acabamos
//!   tecleando una contraseña donde no toca.
//! - Un paso OPCIONAL (la contraseña de ssh) se salta si no llega a tiempo o
//!   si aparece antes lo que espera el paso siguiente. Así el guion también
//!   funciona en servidores donde entras con clave SSH, sin contraseña.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vte4::prelude::*;
use vte4::{Format, Terminal};

pub struct Paso {
    /// Textos que activan el paso (basta uno). Se comparan en minúsculas.
    pub espera: Vec<String>,
    /// Lo que se escribe; después se envía Intro automáticamente.
    pub escribe: String,
    /// Tiempo máximo de espera, contado desde el paso anterior.
    pub plazo: Duration,
    /// Si es opcional, se puede saltar (ver arriba).
    pub opcional: bool,
}

impl Paso {
    /// ¿Esta línea de pantalla activa el paso? (`linea` ya en minúsculas)
    fn coincide(&self, linea: &str) -> bool {
        self.espera.iter().any(|texto| {
            let texto = texto.to_lowercase();
            linea.starts_with(&texto) || linea.ends_with(&texto)
        })
    }
}

/// El estado que comparten las sucesivas llamadas del closure.
struct Estado {
    /// `VecDeque` = cola: sacamos por delante con `pop_front`.
    pendientes: VecDeque<Paso>,
    desde: Instant,
}

impl Estado {
    /// Descarta los pasos caducados. Devuelve `false` si ya no queda nada.
    fn quitar_caducados(&mut self) -> bool {
        loop {
            // Primero sacamos dos datos del paso (copias de un bool), y SOLO
            // después tocamos la cola: mientras tengamos prestado un elemento
            // de la cola, Rust no nos deja modificarla.
            let (caducado, opcional) = match self.pendientes.front() {
                None => return false,
                Some(paso) => (self.desde.elapsed() > paso.plazo, paso.opcional),
            };
            if !caducado {
                return true;
            }
            if opcional {
                self.pendientes.pop_front(); // se salta y se mira el siguiente
                self.desde = Instant::now();
            } else {
                self.pendientes.clear(); // se abandona todo el guion
                return false;
            }
        }
    }

    /// Si la línea activa un paso, lo saca de la cola y lo devuelve.
    fn paso_para(&mut self, linea: &str) -> Option<Paso> {
        // Miramos los dos primeros pasos: `.take(2)` limita el iterador y
        // `.position(...)` dice en qué posición está el primero que cumple.
        let posicion = self
            .pendientes
            .iter()
            .take(2)
            .position(|p| p.coincide(linea))?;
        // El segundo paso solo vale si el primero era opcional.
        if posicion == 1 && !self.pendientes[0].opcional {
            return None;
        }
        // `0..posicion` es un rango: se repite `posicion` veces (0 ó 1).
        for _ in 0..posicion {
            self.pendientes.pop_front();
        }
        self.desde = Instant::now();
        self.pendientes.pop_front()
    }
}

/// Empieza a vigilar `terminal` para ir cumpliendo `pasos`.
pub fn vigilar(terminal: &Terminal, pasos: Vec<Paso>) {
    let estado = Rc::new(RefCell::new(Estado {
        pendientes: VecDeque::from(pasos),
        desde: Instant::now(),
    }));

    // "contents-changed" se emite cada vez que cambia algo en pantalla.
    terminal.connect_contents_changed(move |terminal| {
        let paso = {
            let mut estado = estado.borrow_mut();
            if !estado.quitar_caducados() {
                return;
            }
            let linea = linea_del_cursor(terminal).trim().to_lowercase();
            // `let ... else`: si no hay paso, salimos; si lo hay, queda en `paso`.
            let Some(paso) = estado.paso_para(&linea) else {
                return;
            };
            paso
        }; // aquí se suelta el préstamo, ANTES de escribir en la terminal

        // \r = la tecla Intro tal como la envía un teclado de terminal.
        terminal.feed_child(format!("{}\r", paso.escribe).as_bytes());
    });
}

/// ¿Está el cursor justo después de un prompt, sin nada escrito?
///
/// La línea (sin espacios finales) termina en `$` o `#`, Y el cursor está
/// MÁS ALLÁ de ese carácter. Así distinguimos:
///   "[root@soporte01 ~]# █"   -> prompt vacío (cursor tras el espacio)
///   "$ echo $█"               -> NO: hay un comando a medias acabado en $
pub fn en_prompt_vacio(terminal: &Terminal) -> bool {
    let (columna, _fila) = terminal.cursor_position();
    let linea = linea_del_cursor(terminal);
    let recortada = linea.trim_end();
    let acaba_en_prompt = recortada.ends_with('$') || recortada.ends_with('#');
    acaba_en_prompt && columna as usize > recortada.chars().count()
}

/// ¿Está el cursor en un prompt de ROOT vacío? Como `en_prompt_vacio`, pero
/// solo vale si el prompt acaba en `#` (p. ej. "[root@soporte01 ~]# ").
pub fn en_prompt_root(terminal: &Terminal) -> bool {
    en_prompt_vacio(terminal) && linea_del_cursor(terminal).trim_end().ends_with('#')
}

/// El texto de la línea donde está el cursor.
fn linea_del_cursor(terminal: &Terminal) -> String {
    let (_columna, fila) = terminal.cursor_position();
    let (texto, _longitud) =
        terminal.text_range_format(Format::Text, fila, 0, fila, terminal.column_count());
    // `texto` es opcional: si no hay, devolvemos "" (el valor por defecto de String).
    texto.map(|t| t.to_string()).unwrap_or_default()
}
