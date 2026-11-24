// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! "Mantener sesión abierta": evita que el servidor te desconecte por inactividad.
//!
//! El caso típico es `TMOUT` (p. ej. `TMOUT=900` en /etc/profile): bash cierra
//! la sesión si pasa ese tiempo ESPERANDO EN EL PROMPT sin recibir una línea
//! completa. Por eso:
//!
//! - Cada pestaña con esta opción tiene su propio temporizador, esté visible o
//!   no. Si llevas `intervalo` segundos sin pulsar ninguna tecla en ella...
//! - ...y el cursor está en un PROMPT VACÍO (la línea termina en `$` o `#` y
//!   no hay nada escrito detrás), se envía un Intro: bash ve una línea
//!   (vacía), pinta un prompt nuevo y reinicia su cuenta de TMOUT.
//! - Si estás en vim, less, un `tail -f`... no se envía NADA: ahí TMOUT no
//!   corre, y un Intro podría hacer algo indeseado.
//!
//! (Los cortes de red por inactividad los evita aparte `ServerAliveInterval`,
//! una opción de ssh: ver ventana_terminal.rs.)

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use vte4::gtk::glib;
use vte4::prelude::*;
use vte4::Terminal;

use crate::actividad;
use crate::ajustes;
use crate::autorespuesta;

/// Cada cuántos segundos mira el temporizador si toca hacer algo.
const COMPROBAR_CADA: u32 = 15;

/// Activa "mantener sesión abierta" en una terminal.
///
/// `silencio`: el de la pestaña (actividad.rs). Nuestro Intro automático
/// repinta el prompt, y eso NO debe encender el aviso de "cosas por leer".
pub fn activar(terminal: &Terminal, silencio: actividad::Silencio) {
    // `Cell` = como RefCell, pero para valores pequeños que se COPIAN
    // (como `Instant` o `bool`): `.get()` lee una copia y `.set()` la cambia.
    // No hay préstamos, así que no hay riesgo de "ya está prestado".
    let ultima_tecla = Rc::new(Cell::new(Instant::now()));
    let ultimo_envio = Rc::new(Cell::new(Instant::now()));
    let sesion_viva = Rc::new(Cell::new(true));

    // "commit" = has escrito algo (teclado o pegar). Lo que enviamos nosotros
    // con feed_child NO emite commit, así que no cuenta como actividad tuya.
    {
        let ultima_tecla = ultima_tecla.clone();
        terminal.connect_commit(move |_, _, _| ultima_tecla.set(Instant::now()));
    }
    {
        let sesion_viva = sesion_viva.clone();
        terminal.connect_child_exited(move |_, _| sesion_viva.set(false));
    }

    // REFERENCIA DÉBIL: el temporizador no "posee" la terminal. Si cierras la
    // pestaña, la terminal se destruye igualmente; `upgrade()` dará None y
    // el temporizador se detiene solo. (Con una referencia normal, el
    // temporizador mantendría viva una terminal que ya nadie ve.)
    let terminal_debil = terminal.downgrade();

    glib::timeout_add_seconds_local(COMPROBAR_CADA, move || {
        let Some(terminal) = terminal_debil.upgrade() else {
            return glib::ControlFlow::Break; // pestaña cerrada: se acabó
        };
        if !sesion_viva.get() {
            return glib::ControlFlow::Break; // la sesión terminó
        }

        // El intervalo se lee cada vez: si lo cambias en Ajustes, vale al momento.
        let intervalo = Duration::from_secs(ajustes::actuales().intervalo_mantener as u64);
        let inactiva = ultima_tecla.get().elapsed() >= intervalo;
        let toca = ultimo_envio.get().elapsed() >= intervalo;

        if inactiva && toca && autorespuesta::en_prompt_vacio(&terminal) {
            actividad::silenciar(&silencio, 3);
            terminal.feed_child(b"\r");
            ultimo_envio.set(Instant::now());
        }
        glib::ControlFlow::Continue // seguir comprobando
    });
}
