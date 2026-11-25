// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! La ventana de ajustes: fuente, tamaño, fósforo, paleta y efectos CRT.
//!
//! Cada cambio se guarda al momento (ajustes.toml) y se aplica a todas las
//! pestañas abiertas y a las demás ventanas. No hay botón "Aplicar".

use gtk::prelude::*;
use vte4::gtk;
// El preludio de Pango trae los métodos de las familias de fuentes
// (is_monospace, name). Puede que gtk::prelude ya los incluya: por si acaso.
#[allow(unused_imports)]
use gtk::pango::prelude::*;

use crate::acerca;
use crate::ajustes::{self, Ajustes, Fosforo, Salvapantallas};
use crate::bloqueo;
use crate::boveda;
use crate::llavero;
use crate::tema;
use crate::ventana_terminal;

/// Abre la ventana de ajustes (o la trae al frente si ya estaba abierta).
pub fn mostrar(app: &gtk::Application) {
    if bloqueo::esta_bloqueado() {
        return;
    }
    if let Some(ventana) = app
        .windows()
        .into_iter()
        .find(|ventana| ventana.has_css_class("preferencias"))
    {
        ventana.present();
        return;
    }

    let actuales = ajustes::actuales();

    let ventana = gtk::Window::builder()
        .application(app)
        .title("RetroTerm — Ajustes")
        .resizable(false)
        .build();
    ventana.add_css_class("retro");
    ventana.add_css_class("preferencias");

    // ---- Fuente: todas las monoespaciadas instaladas.
    let fuentes = fuentes_monoespaciadas(&ventana, &actuales.fuente);
    // `String::as_str` usado como función: igual que `|f| f.as_str()`.
    let nombres: Vec<&str> = fuentes.iter().map(String::as_str).collect();
    let selector_fuente = gtk::DropDown::from_strings(&nombres);
    if let Some(posicion) = fuentes.iter().position(|f| *f == actuales.fuente) {
        selector_fuente.set_selected(posicion as u32);
    }
    {
        let app = app.clone();
        // `fuentes` se MUEVE al closure (ya no la usamos fuera).
        selector_fuente.connect_selected_notify(move |selector| {
            if let Some(fuente) = fuentes.get(selector.selected() as usize) {
                let fuente = fuente.clone();
                ajustes::cambiar(|a| a.fuente = fuente);
                aplicar(&app);
            }
        });
    }

    // ---- Tamaño
    let selector_tamano = gtk::SpinButton::with_range(
        ajustes::TAMANO_MIN as f64,
        ajustes::TAMANO_MAX as f64,
        1.0,
    );
    selector_tamano.set_value(actuales.tamano as f64);
    {
        let app = app.clone();
        selector_tamano.connect_value_changed(move |selector| {
            let tamano = selector.value_as_int() as u32;
            ajustes::cambiar(|a| a.tamano = tamano);
            aplicar(&app);
        });
    }

    // ---- Color del fósforo
    let nombres: Vec<&str> = Fosforo::TODOS.iter().map(|f| f.tema().nombre).collect();
    let selector_fosforo = gtk::DropDown::from_strings(&nombres);
    if let Some(posicion) = Fosforo::TODOS.iter().position(|f| *f == actuales.fosforo) {
        selector_fosforo.set_selected(posicion as u32);
    }
    {
        let app = app.clone();
        selector_fosforo.connect_selected_notify(move |selector| {
            // `.get()` devuelve Option<&Fosforo>; con `copied()` obtenemos
            // Option<Fosforo> (se puede porque Fosforo es `Copy`).
            if let Some(fosforo) = Fosforo::TODOS.get(selector.selected() as usize).copied() {
                ajustes::cambiar(|a| a.fosforo = fosforo);
                aplicar(&app);
            }
        });
    }

    // ---- Casillas. Cada una recibe una pequeña función que dice QUÉ campo
    // de los ajustes cambia. Los closures que no capturan nada se pueden
    // usar como punteros a función (`fn(&mut Ajustes, bool)`).
    let copiar_al_seleccionar = casilla(
        "Copiar al seleccionar (al portapapeles de Ctrl+V)",
        actuales.copiar_al_seleccionar,
        app,
        |a, activa| a.copiar_al_seleccionar = activa,
    );
    let sonido = casilla(
        "Sonidos de aviso (campana de la terminal y de los campos de texto)",
        actuales.sonido,
        app,
        |a, activa| a.sonido = activa,
    );
    let monocromo = casilla(
        "Paleta de fósforo (si no: colores originales)",
        actuales.monocromo,
        app,
        |a, activa| a.monocromo = activa,
    );
    let lineas = casilla(
        "Líneas de barrido",
        actuales.crt_lineas,
        app,
        |a, activa| a.crt_lineas = activa,
    );
    let vineta = casilla(
        "Viñeteado (bordes oscuros)",
        actuales.crt_vineta,
        app,
        |a, activa| a.crt_vineta = activa,
    );
    // Deslizador de 0.0 a 1.0, en pasos de 0.05.
    let intensidad = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
    intensidad.set_value(actuales.crt_intensidad);
    intensidad.set_hexpand(true);
    {
        let app = app.clone();
        intensidad.connect_value_changed(move |deslizador| {
            let valor = deslizador.value();
            ajustes::cambiar(|a| a.crt_intensidad = valor);
            aplicar(&app);
        });
    }
    let halo = casilla(
        "Halo del fósforo (resplandor de las letras)",
        actuales.crt_halo,
        app,
        |a, activa| a.crt_halo = activa,
    );
    // Deslizador de la fuerza del halo, como el de las líneas.
    let intensidad_halo = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
    intensidad_halo.set_value(actuales.crt_halo_intensidad);
    intensidad_halo.set_hexpand(true);
    {
        let app = app.clone();
        intensidad_halo.connect_value_changed(move |deslizador| {
            let valor = deslizador.value();
            ajustes::cambiar(|a| a.crt_halo_intensidad = valor);
            aplicar(&app);
        });
    }
    let esquinas = casilla(
        "Marco con esquinas redondeadas",
        actuales.crt_esquinas,
        app,
        |a, activa| a.crt_esquinas = activa,
    );

    // ---- Sesiones: segundos sin teclear antes de "Mantener sesión abierta".
    let intervalo_mantener = gtk::SpinButton::with_range(30.0, 3600.0, 30.0);
    intervalo_mantener.set_value(actuales.intervalo_mantener as f64);
    {
        let app = app.clone();
        intervalo_mantener.connect_value_changed(move |selector| {
            let segundos = selector.value_as_int() as u32;
            ajustes::cambiar(|a| a.intervalo_mantener = segundos);
            aplicar(&app);
        });
    }
    let ayuda_mantener = gtk::Label::builder()
        .label("Segundos sin teclear. Debe ser menor que el TMOUT del servidor.")
        .xalign(0.0)
        .wrap(true)
        .build();
    ayuda_mantener.add_css_class("ayuda");

    // ---- KeePass: bloqueo automático por inactividad.
    let keepass_auto = gtk::SpinButton::with_range(0.0, 240.0, 5.0);
    keepass_auto.set_value(actuales.keepass_auto_min as f64);
    keepass_auto.connect_value_changed(|selector| {
        let minutos = selector.value_as_int() as u32;
        ajustes::cambiar(|a| a.keepass_auto_min = minutos);
        // Si está abierta, el nuevo plazo cuenta desde ahora.
        boveda::reiniciar_temporizador();
    });
    let ayuda_keepass_auto = gtk::Label::builder()
        .label("Minutos sin usar sus contraseñas (0 = nunca). También se bloquea al bloquear RetroTerm.")
        .xalign(0.0)
        .wrap(true)
        .build();
    ayuda_keepass_auto.add_css_class("ayuda");

    // ---- KeePass: olvidar la contraseña maestra guardada en el llavero.
    let boton_olvidar = gtk::Button::with_label("Olvidar contraseña maestra guardada");
    boton_olvidar.set_halign(gtk::Align::Start);
    let estado_keepass = gtk::Label::builder().xalign(0.0).wrap(true).build();
    estado_keepass.add_css_class("ayuda");
    {
        let estado_keepass = estado_keepass.clone();
        boton_olvidar.connect_clicked(move |_| {
            // `let ... else`: sin fichero KeePass conocido, no hay nada que olvidar.
            let Some(fichero) = ajustes::actuales().keepass else {
                estado_keepass.set_text("No hay ningún fichero KeePass configurado.");
                return;
            };
            match llavero::borrar_maestra(&fichero) {
                Ok(()) => estado_keepass
                    .set_text("Hecho: la próxima vez se pedirá la contraseña maestra."),
                Err(error) => estado_keepass.set_text(&format!("Llavero no disponible: {error}")),
            }
        });
    }

    let aviso_pegar_root = casilla(
        "Avisar antes de pegar varias líneas en una shell de root",
        actuales.aviso_pegar_root,
        app,
        |a, activa| a.aviso_pegar_root = activa,
    );
    let aviso_actividad = casilla(
        "Avisar de actividad en pestañas en segundo plano",
        actuales.aviso_actividad,
        app,
        |a, activa| a.aviso_actividad = activa,
    );

    // ---- Bloqueo
    let bloqueo_activado = casilla(
        "Botón candado y Ctrl+Mayús+L para bloquear",
        actuales.bloqueo_activado,
        app,
        |a, activa| a.bloqueo_activado = activa,
    );
    let bloqueo_auto = gtk::SpinButton::with_range(0.0, 240.0, 5.0);
    bloqueo_auto.set_value(actuales.bloqueo_auto_min as f64);
    {
        let app = app.clone();
        bloqueo_auto.connect_value_changed(move |selector| {
            let minutos = selector.value_as_int() as u32;
            ajustes::cambiar(|a| a.bloqueo_auto_min = minutos);
            aplicar(&app);
        });
    }
    // ---- Salvapantallas: se recuerda en ajustes.toml, así que al abrir
    // Ajustes el desplegable sale con el efecto elegido la última vez.
    let nombres: Vec<&str> = Salvapantallas::TODOS.iter().map(|s| s.nombre()).collect();
    let selector_salvapantallas = gtk::DropDown::from_strings(&nombres);
    if let Some(posicion) = Salvapantallas::TODOS
        .iter()
        .position(|s| *s == actuales.salvapantallas)
    {
        selector_salvapantallas.set_selected(posicion as u32);
    }
    selector_salvapantallas.connect_selected_notify(|selector| {
        if let Some(efecto) = Salvapantallas::TODOS.get(selector.selected() as usize).copied() {
            // No hace falta `aplicar`: el efecto se elige al bloquear.
            ajustes::cambiar(|a| a.salvapantallas = efecto);
        }
    });

    let ayuda_bloqueo = gtk::Label::builder()
        .label(
            "Minutos sin tocar RetroTerm (0 = nunca). Para desbloquear: tu contraseña de sesión.",
        )
        .xalign(0.0)
        .wrap(true)
        .build();
    ayuda_bloqueo.add_css_class("ayuda");

    // ---- Restablecer: vuelve a los valores por defecto y reabre la ventana
    // (así todos los controles muestran los valores nuevos).
    let boton_restablecer = gtk::Button::with_label("Valores por defecto");

    {
        let (app, ventana) = (app.clone(), ventana.clone());
        boton_restablecer.connect_clicked(move |_| {
            // `*a = ...` sustituye el contenido entero de lo que apunta `a`.
            ajustes::cambiar(|a| *a = Ajustes::default());
            aplicar(&app);
            ventana.close();
            // La reabrimos "en cuanto GTK esté libre", cuando la vieja ya
            // se haya ido del todo (si no, mostrar() la encontraría).
            let app = app.clone();
            gtk::glib::idle_add_local_once(move || mostrar(&app));
        });
    }

    // ---- Montaje: secciones con título y filas "etiqueta | control".
    let rejilla = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .build();
    let mut fila = 0;
    // Un closure que modifica `fila` y `rejilla`: por eso es `mut`.
    let mut seccion = |titulo: &str, filas: &[(&str, &gtk::Widget)]| {
        let cabecera = gtk::Label::builder().label(titulo).xalign(0.0).build();
        cabecera.add_css_class("seccion");
        rejilla.attach(&cabecera, 0, fila, 2, 1); // ocupa las 2 columnas
        fila += 1;
        for (texto, control) in filas {
            let etiqueta = gtk::Label::builder().label(*texto).xalign(1.0).build();
            rejilla.attach(&etiqueta, 0, fila, 1, 1);
            rejilla.attach(*control, 1, fila, 1, 1);
            fila += 1;
        }
    };
    seccion(
        "▒ Texto",
        &[
            ("Fuente", selector_fuente.upcast_ref()),
            ("Tamaño", selector_tamano.upcast_ref()),
            ("", copiar_al_seleccionar.upcast_ref()),
            ("", sonido.upcast_ref()),
        ],
    );
    seccion(
        "▒ Color",
        &[
            ("Fósforo", selector_fosforo.upcast_ref()),
            ("", monocromo.upcast_ref()),
        ],
    );
    seccion(
        "▒ Pantalla CRT",
        &[
            ("", lineas.upcast_ref()),
            ("Intensidad", intensidad.upcast_ref()),
            ("", vineta.upcast_ref()),
            ("", esquinas.upcast_ref()),
            ("", halo.upcast_ref()),
            ("Intensidad del halo", intensidad_halo.upcast_ref()),
        ],
    );
    seccion(
        "▒ Bloqueo",
        &[
            ("", bloqueo_activado.upcast_ref()),
            ("Automático", bloqueo_auto.upcast_ref()),
            ("Salvapantallas", selector_salvapantallas.upcast_ref()),
            ("", ayuda_bloqueo.upcast_ref()),
        ],
    );
    seccion(
        "▒ KeePass",
        &[
            ("Bloqueo automático", keepass_auto.upcast_ref()),
            ("", ayuda_keepass_auto.upcast_ref()),
            ("", boton_olvidar.upcast_ref()),
            ("", estado_keepass.upcast_ref()),
        ],
    );
    seccion(
        "▒ Sesiones",
        &[
            ("Mantener abierta", intervalo_mantener.upcast_ref()),
            ("", ayuda_mantener.upcast_ref()),
            ("", aviso_actividad.upcast_ref()),
            ("", aviso_pegar_root.upcast_ref()),
        ],
    );

    let contenido = gtk::Box::new(gtk::Orientation::Vertical, 16);
    contenido.set_margin_top(20);
    contenido.set_margin_bottom(20);
    contenido.set_margin_start(24);
    contenido.set_margin_end(24);
    // Las secciones van en una zona con barra de desplazamiento: si la
    // ventana no cabe en la pantalla, se desplaza en vez de pedir más alto
    // del que hay (avisos «Trying to measure GtkWindow… needs at least…»).
    // `propagate_natural_*`: si SÍ cabe, la ventana toma el tamaño de todo el
    // contenido y no aparece ninguna barra. Los botones quedan fuera, fijos.
    let desplazable = gtk::ScrolledWindow::builder()
        .child(&rejilla)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_width(true)
        .propagate_natural_height(true)
        .vexpand(true)
        .build();
    contenido.append(&desplazable);
    // "Acerca de" a la izquierda y "Valores por defecto" a la derecha.
    let boton_acerca = gtk::Button::with_label("Acerca de RetroTerm");
    {
        let ventana = ventana.clone();
        boton_acerca.connect_clicked(move |_| acerca::mostrar(Some(&ventana)));
    }
    // "Aceptar" solo cierra: cada cambio ya se guardó y aplicó al momento.
    let boton_aceptar = gtk::Button::with_label("Aceptar");
    {
        // Referencia débil: el closure vive dentro del botón, que está en
        // la propia ventana (una normal formaría un ciclo; ver CLAUDE.md).
        let ventana = ventana.downgrade();
        boton_aceptar.connect_clicked(move |_| {
            if let Some(ventana) = ventana.upgrade() {
                ventana.close();
            }
        });
    }
    let separador = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    separador.set_hexpand(true); // ocupa el hueco del medio
    let botones = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    botones.append(&boton_acerca);
    botones.append(&separador);
    botones.append(&boton_restablecer);
    botones.append(&boton_aceptar);
    contenido.append(&botones);

    ventana.set_child(Some(&contenido));
    // Botón por defecto: se ve resaltado y lo pulsa Intro cuando el foco
    // está en un control que reenvía el Intro.
    ventana.set_default_widget(Some(&boton_aceptar));
    ventana.present();
}

/// Una casilla que, al marcarla o desmarcarla, cambia un ajuste.
///
/// `cambio: fn(&mut Ajustes, bool)` = un PUNTERO A FUNCIÓN: recibimos una
/// función como parámetro, igual que recibiríamos un número.
fn casilla(
    texto: &str,
    activa: bool,
    app: &gtk::Application,
    cambio: fn(&mut Ajustes, bool),
) -> gtk::CheckButton {
    let casilla = gtk::CheckButton::with_label(texto);
    casilla.set_active(activa);
    let app = app.clone();
    casilla.connect_toggled(move |casilla| {
        let activa = casilla.is_active();
        ajustes::cambiar(|a| cambio(a, activa));
        aplicar(&app);
    });
    casilla
}

/// Aplica los ajustes actuales a todo: CSS de las ventanas y terminales.
fn aplicar(app: &gtk::Application) {
    tema::recargar_estilos();
    ventana_terminal::aplicar_ajustes(app);
}

/// Los nombres de las fuentes monoespaciadas instaladas, ordenados.
/// Si la fuente actual no aparece (p. ej. no está instalada), va la primera.
fn fuentes_monoespaciadas(widget: &impl IsA<gtk::Widget>, actual: &str) -> Vec<String> {
    let mut fuentes: Vec<String> = widget
        .pango_context()
        .list_families()
        .iter()
        .filter(|familia| familia.is_monospace())
        .map(|familia| familia.name().to_string())
        .collect();
    fuentes.sort();
    fuentes.dedup(); // quita repetidos consecutivos (por eso se ordena antes)

    // `iter().any(...)` = "¿hay alguna igual?"
    if !fuentes.iter().any(|f| f == actual) {
        fuentes.insert(0, actual.to_string());
    }
    fuentes
}
