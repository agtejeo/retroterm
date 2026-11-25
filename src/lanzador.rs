// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Ventana de inicio: elegir/crear un perfil y conectarse.
//!
//! Aquí aparecen dos piezas clave de Rust con GTK: `Rc` y `RefCell`.
//!
//! - `Rc<T>` ("reference counted") permite que VARIOS dueños compartan el
//!   mismo dato. Cada `.clone()` de un Rc NO copia el dato: solo suma uno
//!   al contador. El dato se libera cuando el último dueño desaparece.
//!   Lo necesitamos porque cada botón (cada closure) quiere acceso a la
//!   lista de perfiles.
//!
//! - `RefCell<T>` permite MODIFICAR un dato compartido. Normalmente Rust
//!   comprueba los préstamos al compilar; RefCell lo comprueba al ejecutar:
//!   `.borrow()` presta para leer, `.borrow_mut()` presta para escribir, y
//!   si pides escribir mientras alguien lee, el programa se detiene (panic).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use vte4::gtk;

use crate::ajustes::{self, UltimaConexion};
use crate::bloqueo;
use crate::boveda;
use crate::desbloqueo;
use crate::llavero;
use crate::perfil::{self, Perfil};
use crate::preferencias;
use crate::sesion;
use crate::ventana_terminal;

const TEXTO_GUARDADA: &str = "(guardada en el llavero)";

/// Las dos entradas fijas del desplegable de perfiles van SIEMPRE al final,
/// detrás de los n perfiles guardados: así las posiciones 0..n siguen
/// siendo las de los perfiles y "Guardar"/"Borrar" no necesitan cambiar.
///
/// Posición n: abrir tu shell local.
const TEXTO_LOCAL: &str = "localhost · sesión local";
/// Posición n+1: formulario vacío para crear un perfil.
///
/// ¿Por qué una entrada y no «nada elegido»? El `DropDown` de GTK lleva
/// dentro una `SingleSelection` con «autoselect»: `set_selected(INVALID_
/// LIST_POSITION)` se IGNORA y siempre queda algo elegido. Sin perfiles, ese
/// algo era «localhost», y no había forma de crear el primero.
const TEXTO_NUEVO: &str = "+ Perfil nuevo";
/// Aviso que se muestra con "localhost" elegido.
const AVISO_LOCAL: &str = "Sesión local: «Conectar» abre tu shell en una pestaña nueva.";

/// Guion que se propone al elegir una entrada de KeePass con el guion vacío.
const GUION_ROOT: &str = "$ => su -\nPassword:|Contraseña: => {root}";

/// `type` crea un alias: un nombre corto para un tipo largo.
type Perfiles = Rc<RefCell<Vec<Perfil>>>;

/// Los campos del formulario, agrupados para pasarlos juntos.
struct Formulario {
    nombre: gtk::Entry,
    servidor: gtk::Entry,
    usuario: gtk::Entry,
    puerto: gtk::Entry,
    /// Comando de un perfil local (vacío = perfil SSH).
    comando: gtk::Entry,
    clave: gtk::PasswordEntry, // como Entry, pero muestra ●●●● (con un ojo para ver)
    recordar: gtk::CheckButton,
    guion: gtk::TextView, // texto de varias líneas
    /// Entrada de KeePass (desplegable) y lo que vale cada opción: `None` =
    /// ninguna, `Some(ruta)` = esa entrada.
    keepass: gtk::DropDown,
    opciones_keepass: RefCell<Vec<Option<String>>>,
    boton_boveda: gtk::Button,  // "Desbloquear…" / "Bloquear"
    cambios: gtk::CheckButton,  // "Aplicar cambios a caracteres"
    mantener: gtk::CheckButton, // "Mantener sesión abierta"
    estado: gtk::Label,         // línea de avisos debajo de los botones
}

impl Formulario {
    /// `Self` = el propio tipo (Formulario). Por convención, el constructor
    /// se llama `new`; aquí, `nuevo`.
    fn nuevo() -> Self {
        // Un closure guardado en una variable, para no repetir el builder 4 veces.
        let campo = |ejemplo: &str| {
            gtk::Entry::builder()
                .placeholder_text(ejemplo)
                .hexpand(true)
                .activates_default(true) // Intro = pulsar el botón por defecto (Conectar)
                .build()
        };
        let formulario = Formulario {
            nombre: campo("servidor01"),
            servidor: campo("servidor01.midominio.es"),
            usuario: campo("usuario"),
            puerto: campo("22"),
            comando: campo("vacío = conexión SSH · p. ej.: sudo openconnect …"),
            clave: gtk::PasswordEntry::builder()
                .show_peek_icon(true)
                .hexpand(true)
                .activates_default(true)
                .build(),
            recordar: gtk::CheckButton::with_label("Recordar contraseña en el llavero"),
            guion: gtk::TextView::builder()
                .monospace(true)
                .accepts_tab(false) // Tab salta al siguiente campo
                .top_margin(4)
                .bottom_margin(4)
                .left_margin(6)
                .right_margin(6)
                .build(),
            keepass: gtk::DropDown::builder().hexpand(true).build(),
            opciones_keepass: RefCell::new(Vec::new()),
            boton_boveda: gtk::Button::new(),
            cambios: gtk::CheckButton::with_label("Aplicar cambios a caracteres"),
            mantener: gtk::CheckButton::with_label("Mantener sesión abierta (contra TMOUT)"),
            estado: gtk::Label::new(None),
        };
        formulario.puerto.set_text("22");
        formulario.cambios.set_active(true); // marcada por defecto
        formulario.poner_keepass(None);
        formulario.estado.add_css_class("aviso");
        formulario
    }

    /// Lee y valida el formulario. Devuelve el perfil o un mensaje de error.
    fn leer(&self) -> Result<Perfil, String> {
        // `.text()` devuelve el texto del campo; `.trim()` quita espacios.
        let host = self.servidor.text().trim().to_string();
        let usuario = self.usuario.text().trim().to_string();
        let nombre = self.nombre.text().trim().to_string();
        let puerto_texto = self.puerto.text();
        let comando = self.comando.text().trim().to_string();
        // Con comando, el perfil es local: no necesita servidor ni usuario.
        let comando_local = if comando.is_empty() {
            None
        } else {
            Some(comando)
        };

        // Perfil local: valores de relleno donde falten (no se usan para conectar).
        let (host, usuario) = if comando_local.is_some() {
            (
                if host.is_empty() { String::from("localhost") } else { host },
                if usuario.is_empty() { sesion::usuario() } else { usuario },
            )
        } else {
            if host.is_empty() {
                return Err(String::from("Falta el servidor."));
            }
            if usuario.is_empty() {
                return Err(String::from("Falta el usuario."));
            }
            // Ver `perfil::dato_ssh_valido`: nada que ssh pueda tomar por una opción.
            if !perfil::dato_ssh_valido(&host) {
                return Err(String::from(
                    "Servidor no válido: sin espacios y sin empezar por «-».",
                ));
            }
            if !perfil::dato_ssh_valido(&usuario) {
                return Err(String::from(
                    "Usuario no válido: sin espacios y sin empezar por «-».",
                ));
            }
            (host, usuario)
        };
        // `.parse()` convierte texto en número; puede fallar, así que da un Result.
        let puerto: u16 = match puerto_texto.trim().parse() {
            Ok(numero) => numero,
            // En un perfil local el puerto da igual: 22 si no hay uno válido.
            Err(_) if comando_local.is_some() => 22,
            Err(_) => return Err(format!("Puerto no válido: «{puerto_texto}».")),
        };
        // `if` en Rust es una EXPRESIÓN: devuelve un valor.
        let nombre = if nombre.is_empty() {
            host.clone()
        } else {
            nombre
        };
        // "localhost" como nombre de un perfil SSH se confunde con la entrada
        // fija «localhost · sesión local». `eq_ignore_ascii_case` compara sin
        // distinguir mayúsculas (Localhost, LOCALHOST… también cuentan).
        // El servidor sí puede ser localhost, con otro nombre de perfil.
        if comando_local.is_none() && nombre.eq_ignore_ascii_case("localhost") {
            return Err(String::from(
                "El nombre «localhost» está reservado para la sesión local: \
                 pon otro nombre al perfil (p. ej. «localhost-ssh»).",
            ));
        }

        // El `?` aquí: si el guion tiene un error, leer() devuelve ese Err.
        let guion = perfil::texto_a_guion(&self.texto_guion())?;

        // Cuando la variable se llama igual que el campo, basta con el nombre.
        Ok(Perfil {
            nombre,
            host,
            usuario,
            puerto,
            guion,
            keepass: self.keepass_elegida(),
            cambios_root: self.cambios.is_active(),
            mantener_sesion: self.mantener.is_active(),
            comando_local,
        })
    }

    /// Rellena el desplegable de KeePass y selecciona `actual`.
    ///
    /// Opciones: "(ninguna)", las entradas de root de la bóveda (si está
    /// abierta) y, si el perfil apunta a una entrada que no está en la lista
    /// (bóveda bloqueada), esa también, para no perderla al guardar.
    fn poner_keepass(&self, actual: Option<&str>) {
        let abierta = boveda::abierta();
        let mut opciones: Vec<Option<String>> = vec![None];
        let mut textos: Vec<String> = vec![String::from(if abierta {
            "(ninguna)"
        } else {
            "(ninguna · KeePass bloqueado)"
        })];

        for (ruta, host) in boveda::lista() {
            textos.push(format!("{ruta}   → {host}"));
            opciones.push(Some(ruta));
        }
        if let Some(actual) = actual {
            // `.contains(&Some(...))`: ¿está ya en la lista?
            if !opciones.contains(&Some(actual.to_string())) {
                textos.push(format!("{actual}   (bloqueada)"));
                opciones.push(Some(actual.to_string()));
            }
        }

        let seleccion = opciones
            .iter()
            .position(|opcion| opcion.as_deref() == actual)
            .unwrap_or(0);

        // Guardamos las opciones ANTES de cambiar el modelo: set_model avisa
        // al closure de "selected", que las lee (ver la trampa de RefCell
        // explicada en "Borrar").
        *self.opciones_keepass.borrow_mut() = opciones;
        let textos: Vec<&str> = textos.iter().map(String::as_str).collect();
        self.keepass.set_model(Some(&gtk::StringList::new(&textos)));
        self.keepass.set_selected(seleccion as u32);
        // La casilla de cambios solo tiene sentido con una entrada elegida.
        self.cambios.set_sensitive(seleccion != 0);

        self.boton_boveda.set_label(if abierta {
            "Bloquear"
        } else {
            "Desbloquear…"
        });
    }

    /// La entrada de KeePass elegida en el desplegable (o None).
    fn keepass_elegida(&self) -> Option<String> {
        self.opciones_keepass
            .borrow()
            .get(self.keepass.selected() as usize)
            .cloned() // de Option<&Option<String>> a Option<Option<String>>
            .flatten() // y de ahí a Option<String>
    }

    /// Todo el texto del cuadro del guion.
    /// Un TextView guarda el texto en un "buffer"; se lee entre dos posiciones.
    fn texto_guion(&self) -> String {
        let buffer = self.guion.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    fn rellenar(&self, perfil: &Perfil) {
        self.nombre.set_text(&perfil.nombre);
        self.servidor.set_text(&perfil.host);
        self.usuario.set_text(&perfil.usuario);
        self.puerto.set_text(&perfil.puerto.to_string());
        // `unwrap_or("")`: si no hay comando (perfil SSH), el campo queda vacío.
        self.comando
            .set_text(perfil.comando_local.as_deref().unwrap_or(""));
        self.guion
            .buffer()
            .set_text(&perfil::guion_a_texto(&perfil.guion));
        self.cambios.set_active(perfil.cambios_root);
        self.mantener.set_active(perfil.mantener_sesion);
        self.poner_keepass(perfil.keepass.as_deref());

        // ¿Hay contraseña guardada para este perfil? No la mostramos: solo
        // marcamos la casilla y lo indicamos en el campo vacío.
        match llavero::leer(perfil) {
            Ok(guardada) => self.marcar_clave(guardada.is_some(), guardada.is_some()),
            Err(error) => {
                self.marcar_clave(false, false);
                self.avisar(&format!("Llavero no disponible: {error}"));
            }
        }
    }

    /// Activa o desactiva el "modo localhost": con la sesión local no hay
    /// servidor, usuario, contraseña ni guion, así que el formulario se
    /// muestra apagado (no se puede editar).
    fn modo_local(&self, local: bool) {
        if local {
            self.limpiar();
            self.nombre.set_text("localhost");
            self.servidor.set_text("localhost");
            self.usuario.set_text(&sesion::usuario());
            self.puerto.set_text("");
            self.avisar(AVISO_LOCAL);
        } else if self.estado.text() == AVISO_LOCAL {
            // Solo quitamos NUESTRO aviso: otros (p. ej. "Perfil borrado")
            // se quedan.
            self.avisar("");
        }
        // Un array de referencias a widgets distintos: `upcast_ref` los ve a
        // todos como `gtk::Widget`, que es lo único que necesitamos aquí.
        let campos: [&gtk::Widget; 11] = [
            self.nombre.upcast_ref(),
            self.servidor.upcast_ref(),
            self.usuario.upcast_ref(),
            self.puerto.upcast_ref(),
            self.comando.upcast_ref(),
            self.clave.upcast_ref(),
            self.recordar.upcast_ref(),
            self.guion.upcast_ref(),
            self.keepass.upcast_ref(),
            self.boton_boveda.upcast_ref(),
            self.mantener.upcast_ref(),
        ];
        for campo in campos {
            campo.set_sensitive(!local);
        }
        // La casilla de cambios depende además de que haya entrada de KeePass.
        self.cambios
            .set_sensitive(!local && self.keepass_elegida().is_some());
    }

    fn limpiar(&self) {
        self.nombre.set_text("");
        self.servidor.set_text("");
        self.usuario.set_text("");
        self.puerto.set_text("22");
        self.comando.set_text("");
        self.guion.buffer().set_text("");
        self.cambios.set_active(true);
        self.mantener.set_active(false);
        self.poner_keepass(None);
        self.marcar_clave(false, false);
    }

    /// Vacía el campo de contraseña y refleja su estado en la casilla.
    fn marcar_clave(&self, recordar: bool, guardada: bool) {
        self.clave.set_text("");
        self.recordar.set_active(recordar);
        let pista = if guardada { TEXTO_GUARDADA } else { "" };
        // PasswordEntry no tiene un método para esto, pero sí la PROPIEDAD
        // "placeholder-text". `set_property` sirve para cualquier propiedad
        // de un objeto GTK, aunque no tenga su método propio.
        self.clave.set_property("placeholder-text", pista);
    }

    /// Lo escrito en el campo de contraseña y si la casilla está marcada.
    /// Devuelve una tupla: dos valores de una vez.
    fn clave_y_casilla(&self) -> (String, bool) {
        (self.clave.text().to_string(), self.recordar.is_active())
    }

    fn avisar(&self, mensaje: &str) {
        self.estado.set_text(mensaje);
    }
}

/// Construye y muestra la ventana de inicio.
pub fn mostrar(app: &gtk::Application) {
    // Con RetroTerm bloqueado no se abre nada (p. ej. si lo relanzas desde el menú).
    if bloqueo::esta_bloqueado() {
        return;
    }
    // Si ya está abierta, la traemos al frente en vez de abrir otra.
    // `.into_iter().find(...)` busca el primer elemento que cumpla la condición.
    if let Some(ventana) = app
        .windows()
        .into_iter()
        .find(|ventana| ventana.has_css_class("lanzador"))
    {
        ventana.present();
        return;
    }

    let perfiles: Perfiles = Rc::new(RefCell::new(perfil::cargar()));
    let formulario = Rc::new(Formulario::nuevo());

    let selector = gtk::DropDown::builder()
        .model(&modelo(&perfiles.borrow()))
        .hexpand(true)
        .build();


    // ---- Si KeePass se bloquea solo (por tiempo) con esta ventana abierta,
    // que el desplegable y el botón lo reflejen. Referencia DÉBIL: el aviso
    // vive en una global de boveda.rs; una normal mantendría vivo el
    // formulario aunque se cerrara la ventana. `Rc::downgrade` es el
    // `downgrade()` de los Rc (los de GTK son métodos del widget).
    {
        let formulario = Rc::downgrade(&formulario);
        boveda::al_bloquear(move || {
            if let Some(formulario) = formulario.upgrade() {
                let actual = formulario.keepass_elegida();
                formulario.poner_keepass(actual.as_deref());
                formulario.avisar("KeePass bloqueado: contraseñas borradas de la memoria.");
            }
        });
    }

    // ---- Al elegir otro perfil en el desplegable, rellenamos el formulario.
    //
    // Patrón habitual: abrimos un bloque { }, clonamos ahí los Rc que el
    // closure necesita (con el mismo nombre, "sombreando" los originales) y
    // los movemos dentro con `move`. Fuera del bloque, los originales siguen
    // disponibles para el siguiente closure.
    {
        let perfiles = perfiles.clone();
        let formulario = formulario.clone();
        selector.connect_selected_notify(move |selector| {
            aplicar_seleccion(&perfiles, &formulario, selector);
        });
    }

    // Al abrir, sale elegida la última conexión usada (ver `posicion_inicial`).
    // `set_selected` solo avisa (y rellena el formulario) si la posición
    // CAMBIA; el desplegable empieza en 0, así que en ese caso lo hacemos a mano.
    let inicial = posicion_inicial(&perfiles.borrow());
    if selector.selected() == inicial {
        aplicar_seleccion(&perfiles, &formulario, &selector);
    } else {
        selector.set_selected(inicial);
    }

    // ---- Rejilla: etiqueta a la izquierda, campo a la derecha.
    let rejilla = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .build();
    // Un array de tuplas (texto, widget). `upcast_ref` ve cada widget como
    // un `gtk::Widget` genérico, para que todos quepan en el mismo array.
    // El cuadro del guion va dentro de un ScrolledWindow (barra de
    // desplazamiento si hay muchas líneas) y con una línea de ayuda debajo.
    let marco_guion = gtk::ScrolledWindow::builder()
        .child(&formulario.guion)
        .min_content_height(96)
        .hexpand(true)
        .build();
    marco_guion.add_css_class("guion");
    let ayuda_guion = gtk::Label::builder()
        .label(format!(
            "Un paso por línea:  espera {sep} escribe\n\
             Ej.:  $ {sep} sudo -i        [sudo] {sep} {{clave}}\n\
             {{root}} = contraseña de KeePass · varias esperas: Password:|Contraseña:",
            sep = perfil::SEPARADOR
        ))
        .xalign(0.0)
        .build();
    ayuda_guion.add_css_class("ayuda");

    // KeePass: desplegable + botón de desbloquear/bloquear en la misma fila.
    let fila_keepass = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fila_keepass.append(&formulario.keepass);
    fila_keepass.append(&formulario.boton_boveda);

    let filas: [(&str, &gtk::Widget); 13] = [
        ("Perfil", selector.upcast_ref()),
        ("Nombre", formulario.nombre.upcast_ref()),
        ("Servidor", formulario.servidor.upcast_ref()),
        ("Usuario", formulario.usuario.upcast_ref()),
        ("Puerto", formulario.puerto.upcast_ref()),
        ("Comando local", formulario.comando.upcast_ref()),
        ("Contraseña", formulario.clave.upcast_ref()),
        ("", formulario.recordar.upcast_ref()),
        ("Root (KeePass)", fila_keepass.upcast_ref()),
        ("", formulario.cambios.upcast_ref()),
        ("", formulario.mantener.upcast_ref()),
        ("Guion", marco_guion.upcast_ref()),
        ("", ayuda_guion.upcast_ref()),
    ];
    // `.enumerate()` da (posición, elemento); desestructuramos la tupla interna.
    for (fila, (texto, widget)) in filas.iter().enumerate() {
        let etiqueta = gtk::Label::builder().label(*texto).xalign(1.0).build();
        rejilla.attach(&etiqueta, 0, fila as i32, 1, 1);
        rejilla.attach(*widget, 1, fila as i32, 1, 1);
    }

    // ---- KeePass: al ELEGIR una entrada, si el servidor o el guion están
    // vacíos, los rellenamos con lo típico (si no, no tocamos lo que hay).
    {
        // Sacamos el desplegable a su propia variable: no podemos pedir
        // prestado `formulario.keepass` y a la vez mover `formulario` al closure.
        let desplegable = formulario.keepass.clone();
        let formulario = formulario.clone();
        desplegable.connect_selected_notify(move |_| {
            let elegida = formulario.keepass_elegida();
            formulario.cambios.set_sensitive(elegida.is_some());
            let Some(ruta) = elegida else {
                return;
            };
            if formulario.servidor.text().trim().is_empty() {
                // `find` sobre la lista (ruta, host) de la bóveda.
                if let Some((_, host)) = boveda::lista().into_iter().find(|(r, _)| *r == ruta) {
                    formulario.servidor.set_text(&host);
                }
            }
            if formulario.texto_guion().trim().is_empty() {
                formulario.guion.buffer().set_text(GUION_ROOT);
            }
        });
    }
    {
        let boton_boveda = formulario.boton_boveda.clone();
        let (app, formulario) = (app.clone(), formulario.clone());
        boton_boveda.connect_clicked(move |boton| {
            if boveda::abierta() {
                // El aviso registrado arriba (`al_bloquear`) refresca el formulario.
                boveda::bloquear();
            } else {
                // Solo desbloquear: después no hay nada más que hacer (`|| {}`).
                pedir_desbloqueo(&app, boton, &formulario, || {});
            }
        });
    }

    // ---- Botones
    let boton_local = gtk::Button::with_label("Terminal local");
    let boton_borrar = gtk::Button::with_label("Borrar");
    let boton_renombrar = gtk::Button::with_label("Renombrar…");
    boton_renombrar.set_tooltip_text(Some(
        "Cambiar el nombre del perfil elegido (para hacer una copia, \
         cambia el nombre y pulsa Guardar)",
    ));
    let boton_guardar = gtk::Button::with_label("Guardar");
    let boton_conectar = gtk::Button::with_label("Conectar");
    let boton_ajustes = gtk::Button::with_label("Ajustes");
    let botones = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    botones.set_halign(gtk::Align::End);
    for boton in [
        &boton_ajustes,
        &boton_local,
        &boton_borrar,
        &boton_renombrar,
        &boton_guardar,
        &boton_conectar,
    ] {
        botones.append(boton);
    }

    {
        let app = app.clone();
        boton_ajustes.connect_clicked(move |_| preferencias::mostrar(&app));
    }

    {
        let app = app.clone();
        boton_local.connect_clicked(move |boton| {
            recordar_conexion(UltimaConexion::Local);
            ventana_terminal::abrir_local(&app);
            cerrar_lanzador(boton);
        });
    }

    {
        let (perfiles, formulario, selector) =
            (perfiles.clone(), formulario.clone(), selector.clone());
        boton_guardar.connect_clicked(move |_| {
            // Leemos contraseña y casilla ANTES de guardar(): guardar() refresca
            // el desplegable, eso vuelve a rellenar el formulario y lo vaciaría.
            let (escrita, recordar) = formulario.clave_y_casilla();
            if let Some(perfil) = guardar(&perfiles, &formulario, &selector) {
                formulario.avisar(&format!("Perfil «{}» guardado.", perfil.nombre));
                aplicar_clave(&perfil, &formulario, escrita, recordar);
            }
        });
    }

    {
        let (app, perfiles, formulario, selector) = (
            app.clone(),
            perfiles.clone(),
            formulario.clone(),
            selector.clone(),
        );
        boton_conectar.connect_clicked(move |boton| {
            // Con "localhost" elegido: tu shell, en una pestaña nueva.
            if es_local(&perfiles, &selector) {
                recordar_conexion(UltimaConexion::Local);
                ventana_terminal::abrir_local(&app);
                cerrar_lanzador(boton);
                return;
            }
            // Leemos la contraseña antes de guardar (ver "Guardar").
            let (escrita, recordar) = formulario.clave_y_casilla();
            // Conectar también guarda: así el servidor queda en la lista.
            if let Some(perfil) = guardar(&perfiles, &formulario, &selector) {
                recordar_conexion(UltimaConexion::Perfil(perfil.nombre.clone()));
                let clave = aplicar_clave(&perfil, &formulario, escrita, recordar);
                // ¿El guion necesita {root} y la bóveda está cerrada? Primero se
                // desbloquea y, en cuanto termina, se conecta SOLO: un único clic.
                let usa_root = perfil.guion.iter().any(|p| p.escribe.contains("{root}"));
                if usa_root && !boveda::abierta() {
                    let (app, boton_conectar) = (app.clone(), boton.clone());
                    pedir_desbloqueo(&app.clone(), boton, &formulario, move || {
                        // Un closure `Fn` puede llamarse varias veces: no puede
                        // "gastar" `clave`, así que pasa una copia.
                        ventana_terminal::abrir_perfil(&app, &perfil, clave.clone());
                        cerrar_lanzador(&boton_conectar);
                    });
                    return;
                }
                ventana_terminal::abrir_perfil(&app, &perfil, clave);
                cerrar_lanzador(boton);
            }
        });
    }

    {
        let (perfiles, formulario, selector) =
            (perfiles.clone(), formulario.clone(), selector.clone());
        boton_borrar.connect_clicked(move |_| {
            let indice = selector.selected() as usize;
            // Bloque con préstamo mutable. Devuelve cuántos perfiles quedan,
            // y al cerrar la llave `}` el préstamo se suelta.
            let quedan = {
                let mut lista = perfiles.borrow_mut();
                if indice >= lista.len() {
                    return; // nada seleccionado: salimos del closure
                }
                let borrado = lista.remove(indice);
                // Su contraseña también se va. `let _ =` descarta el resultado a
                // propósito: si el llavero falla aquí, no es grave.
                let _ = llavero::borrar(&borrado);
                if let Err(error) = perfil::guardar(&lista) {
                    formulario.avisar(&format!("No se pudo guardar: {error}"));
                    return;
                }
                formulario.avisar(&format!("Perfil «{}» borrado.", borrado.nombre));
                lista.len()
            };
            // Importante: esto va DESPUÉS de soltar el borrow_mut. Cambiar el
            // modelo dispara "selected_notify", cuyo closure hace borrow(); si
            // aún tuviéramos el borrow_mut, RefCell detendría el programa.
            selector.set_model(Some(&modelo(&perfiles.borrow())));
            if quedan == 0 {
                // Ya no hay perfiles: «+ Perfil nuevo» (formulario vacío).
                // `set_selected` dispara `aplicar_seleccion`, que lo limpia.
                selector.set_selected(posicion_nuevo(0));
            } else {
                selector.set_selected(indice.min(quedan - 1) as u32);
            }
        });
    }

    // ---- Renombrar: un campo emergente bajo el botón (Intro aplica, Esc cancela).
    {
        let (perfiles, formulario, selector) =
            (perfiles.clone(), formulario.clone(), selector.clone());
        boton_renombrar.connect_clicked(move |boton| {
            let indice = selector.selected() as usize;
            // `.map(|p| p.nombre.clone())`: copiamos solo el nombre, y el
            // préstamo `borrow()` se suelta al acabar la línea.
            let Some(actual) = perfiles.borrow().get(indice).map(|p| p.nombre.clone()) else {
                return; // «localhost» o «+ Perfil nuevo»: no hay perfil que renombrar
            };
            let emergente = gtk::Popover::new();
            emergente.set_parent(boton);
            emergente.connect_closed(|emergente| {
                let emergente = emergente.clone();
                gtk::glib::idle_add_local_once(move || emergente.unparent());
            });
            let campo = gtk::Entry::new();
            campo.set_text(&actual);
            campo.set_width_chars(24);
            emergente.set_child(Some(&campo));

            let (perfiles, formulario, selector) =
                (perfiles.clone(), formulario.clone(), selector.clone());
            // Débil: el campo vive DENTRO del menú (evita un ciclo, ver CLAUDE.md).
            let emergente_debil = emergente.downgrade();
            campo.connect_activate(move |campo| {
                let nuevo = campo.text().trim().to_owned();
                if nuevo != actual {
                    match renombrar_perfil(&perfiles, &selector, indice, &nuevo) {
                        Ok(()) => formulario
                            .avisar(&format!("Perfil «{actual}» renombrado a «{nuevo}».")),
                        Err(mensaje) => formulario.avisar(&mensaje),
                    }
                }
                if let Some(emergente) = emergente_debil.upgrade() {
                    emergente.popdown();
                }
            });
            emergente.popup();
            campo.grab_focus();
            campo.select_region(0, -1);
        });
    }
    // «Renombrar…» solo tiene sentido con un perfil elegido.
    {
        let perfiles = perfiles.clone();
        let boton = boton_renombrar.downgrade(); // débil: ver CLAUDE.md
        let actualizar = move |selector: &gtk::DropDown| {
            if let Some(boton) = boton.upgrade() {
                boton.set_sensitive((selector.selected() as usize) < perfiles.borrow().len());
            }
        };
        actualizar(&selector);
        selector.connect_selected_notify(actualizar);
    }

    // ---- Montaje de la ventana
    let titulo = gtk::Label::new(Some("▓▒░ RETROTERM ░▒▓"));
    titulo.add_css_class("titulo");

    let contenido = gtk::Box::new(gtk::Orientation::Vertical, 16);
    contenido.set_margin_top(24);
    contenido.set_margin_bottom(24);
    contenido.set_margin_start(24);
    contenido.set_margin_end(24);
    contenido.append(&titulo);
    contenido.append(&rejilla);
    contenido.append(&botones);
    contenido.append(&formulario.estado);

    let ventana = gtk::ApplicationWindow::builder()
        .application(app)
        .title("RetroTerm — Conexiones")
        .default_width(560)
        .child(&contenido)
        .build();
    ventana.add_css_class("retro"); // activa los estilos de tema::css()
    ventana.add_css_class("lanzador"); // para encontrarla (ver el principio de mostrar)
    ventana.set_default_widget(Some(&boton_conectar));

    // ---- Tras elegir perfil en el desplegable, el foco vuelve a «Conectar»
    // (así Intro conecta). Una señal puede tener varios manejadores: este se
    // añade aquí, y no junto al otro, porque necesita el botón ya creado.
    {
        // Referencia débil al botón: el closure se guarda dentro del
        // desplegable, que está en la misma ventana que el botón (ver CLAUDE.md).
        let boton = boton_conectar.downgrade();
        selector.connect_selected_notify(move |selector| {
            // La señal también salta cuando el programa cambia la selección
            // (Guardar, Borrar). Solo movemos el foco si lo tenía el
            // desplegable o su lista emergente (que es "hija" del desplegable):
            // es decir, si el perfil lo ha elegido la persona.
            // `.and_then(...)`: encadena Options; `None` si falta alguno.
            let lo_eligio_la_persona = selector
                .root()
                .and_then(|raiz| raiz.focus())
                .is_some_and(|foco| foco == *selector || foco.is_ancestor(selector));
            if !lo_eligio_la_persona {
                return;
            }
            // `idle_add_local_once`: ejecuta el closure cuando GTK termine lo
            // que está haciendo. Al cerrarse, la lista emergente devuelve el
            // foco al desplegable; si lo movemos ya, nos lo pisaría.
            let boton = boton.clone(); // clonar un Weak no alarga la vida del botón
            gtk::glib::idle_add_local_once(move || {
                // `upgrade()` da Some(botón) si todavía existe.
                if let Some(boton) = boton.upgrade() {
                    boton.grab_focus();
                }
            });
        });
    }

    ventana.present();
    // Al abrirse, GTK da el foco al primer control (el desplegable), que se
    // queda con el Intro. Lo ponemos en «Conectar»: Intro = conectar.
    // `GtkWindowExt::set_focus(...)` con el nombre del trait delante porque
    // `set_focus` también existe en `RootExt` y sería ambiguo (ver CLAUDE.md).
    // Va DESPUÉS de `present()`, cuando la ventana ya existe en pantalla.
    GtkWindowExt::set_focus(&ventana, Some(&boton_conectar));
}

/// Desbloquea KeePass (con la contraseña maestra guardada o preguntándola);
/// al terminar, refresca el desplegable de KeePass y ejecuta `despues`.
fn pedir_desbloqueo(
    app: &gtk::Application,
    boton: &gtk::Button,
    formulario: &Rc<Formulario>,
    despues: impl Fn() + 'static,
) {
    let padre = boton.root().and_downcast::<gtk::Window>();
    let formulario = formulario.clone();
    formulario.avisar("Abriendo KeePass…");
    desbloqueo::desbloquear(app, padre.as_ref(), move || {
        let actual = formulario.keepass_elegida();
        formulario.poner_keepass(actual.as_deref());
        formulario.avisar(&format!(
            "KeePass desbloqueado: {} entradas de root.",
            boveda::lista().len()
        ));
        despues();
    });
}

/// Cierra la ventana de conexiones (la que contiene `boton`).
/// Se vuelve a abrir con el botón "+" o Ctrl+Mayús+T de la ventana de terminales.
fn cerrar_lanzador(boton: &gtk::Button) {
    // `root()` = el widget raíz del árbol, es decir, la ventana.
    if let Some(ventana) = boton.root().and_downcast::<gtk::Window>() {
        ventana.close();
    }
}

/// Valida el formulario, guarda el perfil en disco y actualiza el desplegable.
/// Devuelve el perfil guardado, o `None` si algo falló (ya avisado en pantalla).
fn guardar(
    perfiles: &Perfiles,
    formulario: &Formulario,
    selector: &gtk::DropDown,
) -> Option<Perfil> {
    if es_local(perfiles, selector) {
        formulario.avisar("La sesión local no se guarda: elige o escribe un perfil.");
        return None;
    }
    let nuevo = match formulario.leer() {
        Ok(perfil) => perfil,
        Err(mensaje) => {
            formulario.avisar(&mensaje);
            return None;
        }
    };

    let resultado = {
        let mut lista = perfiles.borrow_mut();
        let indice = perfil::insertar(&mut lista, nuevo.clone());
        // `.map(...)` transforma el valor de un Ok y deja pasar un Err tal cual.
        perfil::guardar(&lista).map(|_| indice)
    }; // <- aquí se suelta el préstamo mutable (ver comentario en "Borrar")

    match resultado {
        Ok(indice) => {
            selector.set_model(Some(&modelo(&perfiles.borrow())));
            selector.set_selected(indice as u32);
            Some(nuevo)
        }
        Err(error) => {
            formulario.avisar(&format!("No se pudo guardar: {error}"));
            None
        }
    }
}

/// Guarda o borra la contraseña en el llavero según la casilla, y devuelve
/// la contraseña que hay que usar al conectar (si hay alguna).
///
/// | casilla  | campo     | qué hacemos                          |
/// |----------|-----------|--------------------------------------|
/// | marcada  | escrito   | guardar en el llavero y usarla       |
/// | marcada  | vacío     | usar la que ya estaba en el llavero  |
/// | sin marcar | escrito | borrar del llavero; usarla esta vez  |
/// | sin marcar | vacío   | borrar del llavero; ssh te la pedirá |
fn aplicar_clave(
    perfil: &Perfil,
    formulario: &Formulario,
    escrita: String,
    recordar: bool,
) -> Option<String> {
    let usar_escrita = if escrita.is_empty() {
        None
    } else {
        Some(escrita)
    };

    // `match` sobre una tupla: cada brazo es una fila de la tabla de arriba.
    let resultado = match (recordar, usar_escrita) {
        (true, Some(clave)) => llavero::guardar(perfil, &clave).map(|_| Some(clave)),
        (true, None) => llavero::leer(perfil),
        (false, escrita) => llavero::borrar(perfil).map(|_| escrita),
    };

    match resultado {
        Ok(clave) => {
            formulario.marcar_clave(recordar, recordar && clave.is_some());
            clave
        }
        Err(error) => {
            formulario.avisar(&format!("Llavero no disponible: {error}"));
            None
        }
    }
}

/// Cambia el nombre del perfil de la posición `indice` a `nuevo`, sin tocar
/// el resto de sus datos ni su lugar en la lista.
///
/// Además de `perfiles.toml`:
/// - **Llavero:** la contraseña de un perfil SSH va con `usuario@servidor:
///   puerto` (no con el nombre): nada que hacer. La de un perfil LOCAL va
///   con su nombre: se copia al nombre nuevo y se borra la antigua. Solo en
///   memoria, nunca a disco ni a la consola.
/// - **Última conexión:** si era este perfil, se apunta con el nombre nuevo.
///
/// `Result<(), String>`: `Ok(())` = hecho («()» es «nada que devolver»);
/// `Err(texto)` = el aviso para mostrar.
fn renombrar_perfil(
    perfiles: &Perfiles,
    selector: &gtk::DropDown,
    indice: usize,
    nuevo: &str,
) -> Result<(), String> {
    if nuevo.is_empty() {
        return Err(String::from("El nombre no puede quedar vacío."));
    }
    // Copia del perfil tal como está (el préstamo se suelta en esta línea).
    let Some(viejo) = perfiles.borrow().get(indice).cloned() else {
        return Err(String::from("Elige primero un perfil."));
    };
    if nuevo == TEXTO_LOCAL
        || nuevo == TEXTO_NUEVO
        || (!viejo.es_local() && nuevo.eq_ignore_ascii_case("localhost"))
    {
        return Err(format!("El nombre «{nuevo}» está reservado."));
    }
    // `.any(...)`: ¿algún OTRO perfil se llama ya así? (`i != indice` deja
    // fuera al propio, por si solo cambian mayúsculas/minúsculas).
    let repetido = perfiles
        .borrow()
        .iter()
        .enumerate()
        .any(|(i, p)| i != indice && p.nombre == nuevo);
    if repetido {
        return Err(format!("Ya hay un perfil llamado «{nuevo}»."));
    }

    // `..viejo.clone()`: «el resto de campos, como en viejo» (sintaxis de
    // actualización de structs). Solo cambia el nombre.
    let renombrado = Perfil {
        nombre: nuevo.to_owned(),
        ..viejo.clone()
    };

    // 1) Llavero (solo perfiles locales): copiar ANTES de guardar la lista,
    //    para no dejar la contraseña huérfana si el llavero falla.
    let clave_local = if viejo.es_local() {
        match llavero::leer(&viejo) {
            Ok(clave) => clave,
            Err(error) => return Err(format!("Llavero no disponible: {error}")),
        }
    } else {
        None
    };
    if let Some(clave) = &clave_local {
        if let Err(error) = llavero::guardar(&renombrado, clave) {
            return Err(format!("Llavero no disponible: {error}"));
        }
    }

    // 2) La lista y perfiles.toml (préstamo mutable en su propio bloque).
    let guardado = {
        let mut lista = perfiles.borrow_mut();
        lista[indice] = renombrado.clone();
        perfil::guardar(&lista)
    };
    if let Err(error) = guardado {
        // Deshacemos: lista y llavero como estaban.
        perfiles.borrow_mut()[indice] = viejo;
        if clave_local.is_some() {
            let _ = llavero::borrar(&renombrado);
        }
        return Err(format!("No se pudo guardar: {error}"));
    }
    if clave_local.is_some() {
        // `let _ =`: si falla, solo queda una entrada sobrante en el llavero.
        let _ = llavero::borrar(&viejo);
    }

    // 3) Última conexión.
    if ajustes::actuales().ultima_conexion == UltimaConexion::Perfil(viejo.nombre.clone()) {
        recordar_conexion(UltimaConexion::Perfil(renombrado.nombre.clone()));
    }

    // 4) El desplegable, con el perfil renombrado elegido. Fuera de cualquier
    //    préstamo: `set_model` emite señales que leen `perfiles`.
    selector.set_model(Some(&modelo(&perfiles.borrow())));
    selector.set_selected(indice as u32);
    Ok(())
}

/// Convierte la lista de perfiles en la lista de nombres que muestra el desplegable.
fn modelo(lista: &[Perfil]) -> gtk::StringList {
    let mut nombres: Vec<&str> = lista.iter().map(|p| p.nombre.as_str()).collect();
    // Siempre al final y en este orden (ver TEXTO_LOCAL y TEXTO_NUEVO).
    nombres.push(TEXTO_LOCAL);
    nombres.push(TEXTO_NUEVO);
    gtk::StringList::new(&nombres)
}

/// ¿Está elegida la entrada "localhost"? Es la que va justo detrás de los
/// perfiles, así que su posición es igual al número de perfiles.
fn es_local(perfiles: &Perfiles, selector: &gtk::DropDown) -> bool {
    selector.selected() as usize == perfiles.borrow().len()
}

/// Posición de «+ Perfil nuevo»: justo detrás de «localhost».
fn posicion_nuevo(numero_de_perfiles: usize) -> u32 {
    numero_de_perfiles as u32 + 1
}

/// Rellena el formulario según lo elegido en el desplegable.
fn aplicar_seleccion(perfiles: &Perfiles, formulario: &Formulario, selector: &gtk::DropDown) {
    if es_local(perfiles, selector) {
        formulario.modo_local(true);
        return;
    }
    formulario.modo_local(false);
    let indice = selector.selected() as usize; // `as` convierte entre tipos numéricos
    // `get` da `None` si la posición no es de un perfil: es «+ Perfil nuevo»
    // y toca un formulario vacío (y editable, por el `modo_local(false)`).
    // `.cloned()` copia el perfil, y el préstamo `borrow()` se suelta al
    // acabar ESTA línea: así no sigue vivo mientras el formulario emite
    // señales de GTK (ver la trampa de RefCell en CLAUDE.md).
    let elegido = perfiles.borrow().get(indice).cloned();
    match elegido {
        Some(perfil) => formulario.rellenar(&perfil),
        None => formulario.limpiar(),
    }
}

/// Qué posición del desplegable sale elegida al abrir la ventana: la de la
/// última conexión usada; si ese perfil ya no existe, el primero. Si no hay
/// perfiles, «+ Perfil nuevo».
fn posicion_inicial(lista: &[Perfil]) -> u32 {
    // Sin perfiles, SIEMPRE «+ Perfil nuevo», aunque la última conexión
    // fuera la local: lo primero que hay que hacer es crear uno.
    if lista.is_empty() {
        return posicion_nuevo(0);
    }
    let primera = 0;
    // `match` sobre el enum; en `Perfil(nombre)` sacamos el dato de dentro.
    match ajustes::actuales().ultima_conexion {
        UltimaConexion::Local => lista.len() as u32,
        UltimaConexion::Perfil(nombre) => lista
            .iter()
            .position(|p| p.nombre == nombre)
            .map(|i| i as u32)
            .unwrap_or(primera),
        UltimaConexion::Ninguna => primera,
    }
}

/// Apunta en ajustes.toml cuál ha sido la última conexión.
fn recordar_conexion(conexion: UltimaConexion) {
    ajustes::cambiar(|a| a.ultima_conexion = conexion);
}
