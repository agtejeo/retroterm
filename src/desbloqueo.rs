// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Desbloqueo del fichero KeePass con la contraseña maestra.
//!
//! Dos caminos:
//! - Si la contraseña maestra está guardada en el llavero (casilla
//!   "Recordar…", desmarcada por defecto), se desbloquea directamente.
//! - Si no (o si la guardada ya no sirve), se muestra una ventanita para
//!   escribirla.
//!
//! Descifrar un .kdbx tarda (a propósito: así se frenan los ataques por
//! fuerza bruta). Para que la ventana no se congele mientras tanto, el
//! descifrado se hace en OTRO HILO y esperamos el resultado con `async/.await`:
//!
//! - `gio::spawn_blocking(f)` ejecuta `f` en un hilo aparte y devuelve un
//!   "futuro": un valor que estará disponible más adelante.
//! - `glib::spawn_future_local(async { ... })` ejecuta un bloque `async` en
//!   el bucle de eventos de GTK. Dentro, `.await` espera al futuro SIN
//!   bloquear la ventana: mientras tanto, GTK sigue pintando y respondiendo.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};
use vte4::gtk;

use crate::ajustes;
use crate::boveda;
use crate::llavero;

/// Desbloquea KeePass y, si sale bien, llama a `al_abrir`.
///
/// Usa la contraseña maestra del llavero si está guardada; si no, pregunta.
/// `impl Fn() + 'static` = cualquier función sin parámetros que pueda vivir
/// todo lo necesario (la guardaremos dentro de closures de GTK).
pub fn desbloquear(
    app: &gtk::Application,
    padre: Option<&gtk::Window>,
    al_abrir: impl Fn() + 'static,
) {
    // `Rc<dyn Fn()>`: `al_abrir` puede ser cualquier closure; `dyn Fn()` es
    // "algún tipo de función, decidido al ejecutar". En un Rc para poder
    // compartirlo entre los distintos caminos.
    let al_abrir: Rc<dyn Fn()> = Rc::new(al_abrir);

    // ¿Conocemos el fichero Y hay contraseña maestra guardada para él?
    // `and_then` encadena: si no hay ruta, ni siquiera se consulta el llavero.
    let guardada =
        ajustes::actuales()
            .keepass
            .and_then(|ruta| match llavero::leer_maestra(&ruta) {
                Ok(Some(maestra)) => Some((ruta, maestra)),
                _ => None,
            });

    let Some((ruta, maestra)) = guardada else {
        mostrar(app, padre, al_abrir, None, false);
        return;
    };

    // Hay contraseña guardada: probamos sin preguntar.
    let app = app.clone();
    // `cloned()`: de Option<&Window> a Option<Window> (una referencia propia,
    // que podemos guardar en el closure).
    let padre = padre.cloned();
    descifrar(expandir(&ruta), maestra, move |resultado| match resultado {
        Ok(()) => al_abrir(),
        Err(mensaje) => {
            // La guardada ya no sirve (¿la cambiaste en KeePassXC?): preguntamos,
            // con la casilla marcada para que se guarde la nueva.
            let aviso = format!("La contraseña guardada no ha funcionado.\n{mensaje}");
            mostrar(&app, padre.as_ref(), al_abrir, Some(aviso), true);
        }
    });
}

/// Descifra el .kdbx en otro hilo y, al terminar, deja la bóveda abierta
/// y recuerda la ruta. Después llama a `al_terminar` con el resultado.
///
/// `impl FnOnce(...)`: se llama UNA sola vez, así que puede consumir
/// (mover) lo que haya capturado.
fn descifrar(
    ruta: PathBuf,
    maestra: String,
    al_terminar: impl FnOnce(Result<(), String>) + 'static,
) {
    glib::spawn_future_local(async move {
        // `ruta` y `maestra` se mueven al otro hilo: necesitamos una copia
        // de la ruta para guardarla después en los ajustes.
        let ruta_hilo = ruta.clone();
        let resultado = gio::spawn_blocking(move || boveda::leer(&ruta_hilo, &maestra)).await;

        // Dos capas de Result: la de fuera dice si el hilo terminó bien; la
        // de dentro, si se pudo descifrar. Las juntamos en una sola.
        let resultado = match resultado {
            Ok(Ok(entradas)) => {
                eprintln!("KeePass desbloqueado: {} entradas de root.", entradas.len());
                boveda::desbloquear(entradas);
                ajustes::cambiar(|a| a.keepass = Some(ruta.display().to_string()));
                Ok(())
            }
            Ok(Err(mensaje)) => Err(mensaje),
            Err(_) => Err(String::from("Error interno al descifrar.")),
        };
        al_terminar(resultado);
    });
}

/// La ventanita de la contraseña maestra.
///
/// `aviso`: un mensaje que mostrar ya al abrir (p. ej. "la guardada no sirvió").
/// `recordar`: cómo aparece la casilla "Recordar…" al abrir.
fn mostrar(
    app: &gtk::Application,
    padre: Option<&gtk::Window>,
    al_abrir: Rc<dyn Fn()>,
    aviso: Option<String>,
    recordar: bool,
) {
    let ventana = gtk::Window::builder()
        .application(app)
        .title("RetroTerm — Desbloquear KeePass")
        .modal(true) // mientras esté abierta, la ventana padre no responde
        .resizable(false)
        .default_width(520)
        .build();
    ventana.set_transient_for(padre); // que aparezca encima de su padre
    ventana.add_css_class("retro");

    let fichero = gtk::Entry::builder()
        .hexpand(true)
        .placeholder_text("~/Documentos/claves.kdbx")
        .activates_default(true)
        .build();
    if let Some(ruta) = ajustes::actuales().keepass {
        fichero.set_text(&ruta);
    }
    let maestra = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .hexpand(true)
        .activates_default(true)
        .build();
    let casilla = gtk::CheckButton::with_label("Recordar contraseña maestra en el llavero");
    casilla.set_active(recordar);
    let estado = gtk::Label::new(aviso.as_deref());
    estado.add_css_class("aviso");
    estado.set_wrap(true);
    let boton = gtk::Button::with_label("Desbloquear");
    boton.set_halign(gtk::Align::End);

    {
        let (ventana, fichero, maestra, casilla, estado) = (
            ventana.clone(),
            fichero.clone(),
            maestra.clone(),
            casilla.clone(),
            estado.clone(),
        );
        boton.connect_clicked(move |boton| {
            let ruta = expandir(fichero.text().trim());
            let clave = maestra.text().to_string();
            if clave.is_empty() {
                estado.set_text("Escribe la contraseña maestra.");
                return;
            }
            let recordar = casilla.is_active();
            estado.set_text("Descifrando…");
            boton.set_sensitive(false); // que no se pulse dos veces

            // Lo que necesita el closure del resultado, clonado para moverlo dentro.
            let (ventana, estado, boton, al_abrir) = (
                ventana.clone(),
                estado.clone(),
                boton.clone(),
                al_abrir.clone(),
            );
            let clave_para_llavero = clave.clone();
            let nombre_fichero = ruta.display().to_string();

            descifrar(ruta, clave, move |resultado| match resultado {
                Ok(()) => {
                    // Casilla marcada: guardar la maestra; desmarcada: olvidarla
                    // (por si estaba guardada de antes).
                    let guardado = if recordar {
                        llavero::guardar_maestra(&nombre_fichero, &clave_para_llavero)
                    } else {
                        llavero::borrar_maestra(&nombre_fichero)
                    };
                    if let Err(error) = guardado {
                        eprintln!("Llavero: no se pudo actualizar la contraseña maestra: {error}");
                    }
                    ventana.close();
                    al_abrir();
                }
                Err(mensaje) => {
                    estado.set_text(&mensaje);
                    boton.set_sensitive(true);
                }
            });
        });
    }

    let rejilla = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .build();
    let filas: [(&str, &gtk::Widget); 3] = [
        ("Fichero", fichero.upcast_ref()),
        ("Contraseña maestra", maestra.upcast_ref()),
        ("", casilla.upcast_ref()),
    ];
    for (fila, (texto, widget)) in filas.iter().enumerate() {
        let etiqueta = gtk::Label::builder().label(*texto).xalign(1.0).build();
        rejilla.attach(&etiqueta, 0, fila as i32, 1, 1);
        rejilla.attach(*widget, 1, fila as i32, 1, 1);
    }

    let contenido = gtk::Box::new(gtk::Orientation::Vertical, 12);
    contenido.set_margin_top(20);
    contenido.set_margin_bottom(20);
    contenido.set_margin_start(24);
    contenido.set_margin_end(24);
    contenido.append(&rejilla);
    contenido.append(&boton);
    contenido.append(&estado);

    ventana.set_child(Some(&contenido));
    ventana.set_default_widget(Some(&boton));
    ventana.present();
    // Si ya conocemos el fichero, el cursor va directo a la contraseña.
    if fichero.text().is_empty() {
        fichero.grab_focus();
    } else {
        maestra.grab_focus();
    }
}

/// "~/algo" -> "/home/usuario/algo".
pub fn expandir(texto: &str) -> PathBuf {
    match texto.strip_prefix("~/") {
        Some(resto) => glib::home_dir().join(resto),
        None => PathBuf::from(texto),
    }
}
