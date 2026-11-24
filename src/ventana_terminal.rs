// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! La ventana de terminales, con una pestaña por sesión (local o SSH).
//!
//! - Solo hay UNA ventana de terminales; cada conexión nueva es una pestaña.
//! - Al terminar la sesión de una pestaña (tu último `exit`), la pestaña se
//!   cierra. Si era la última, se cierra la ventana y termina el programa.
//! - Excepción: si ssh FALLA (contraseña incorrecta, servidor caído...), la
//!   pestaña se queda abierta para que leas el error; se cierra al pulsar una tecla.
//! - Una pestaña en segundo plano que recibe texto muestra un barrido de luz
//!   animado hasta que la abres (actividad.rs).
//! - Botón candado o Ctrl+Mayús+L: bloquear con salvapantallas (bloqueo.rs).
//! - Doble clic en el hueco vacío de la barra de pestañas: nueva conexión.
//! - Atajos: Ctrl+Mayús+T nueva conexión, Ctrl+Mayús+W cerrar pestaña,
//!   Ctrl+Mayús+C copiar, Ctrl+Mayús+V (o Mayús+Insert) pegar,
//!   Ctrl+RePág / Ctrl+AvPág cambiar de pestaña (eso ya lo trae GTK).
//! - Ctrl+, o el botón ⚙: ajustes (fuente, tamaño, colores, efectos).
//! - Clic derecho: menú Copiar / Pegar / Ajustes. Botón central: pegar la selección (VTE).

use gtk::{gdk, gio, glib};
use vte4::gtk;
use vte4::prelude::*;
use vte4::{Format, PtyFlags, Terminal};

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// `crate::` = "desde la raíz de este proyecto". Así usamos otros módulos nuestros.
use crate::acerca;
use crate::actividad;
use crate::ajustes;
use crate::autorespuesta::{self, Paso};
use crate::bloc;
use crate::bloqueo;
use crate::boveda;
use crate::crt;
use crate::lanzador;
use crate::mantener;
use crate::pegado::{self, Origen};
use crate::perfil::Perfil;
use crate::preferencias;
use crate::tema;

/// Abre una terminal con tu shell habitual.
pub fn abrir_local(app: &gtk::Application) {
    // `vec![...]` crea un Vec con esos elementos.
    abrir_pestana(app, "local", vec![shell_del_usuario()], true, None);
}

/// Tu shell habitual (variable SHELL), o bash si no está definida.
fn shell_del_usuario() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| String::from("/bin/bash"))
}

/// Abre la pestaña de un perfil:
/// - perfil SSH: ejecuta `ssh -p PUERTO usuario@host`;
/// - perfil local (con `comando_local`): abre tu shell y teclea el comando.
///
/// Si recibe una contraseña, la escribe cuando ssh la pida (solo en SSH) y
/// la deja disponible como `{clave}` para el guion. El resto de preguntas
/// (como el código OTP) las respondes tú.
pub fn abrir_perfil(app: &gtk::Application, perfil: &Perfil, clave: Option<String>) {
    // `match` sobre `&Option<String>`: `Some(orden)` toma prestado el texto.
    let (terminal, silencio) = match &perfil.comando_local {
        Some(orden) => abrir_pestana(
            app,
            &perfil.nombre,
            vec![shell_del_usuario()],
            true, // es local: al salir de la shell la pestaña se cierra sin más
            Some(orden.clone()),
        ),
        None => {
            let mut comando = vec![
                String::from("ssh"),
                String::from("-p"),
                perfil.puerto.to_string(),
            ];
            if perfil.mantener_sesion {
                // Un "latido" invisible cada 60 s a nivel de protocolo SSH:
                // evita que cortafuegos o NAT corten la conexión por inactividad.
                // `extend` añade varios elementos de golpe al final del Vec.
                comando.extend([String::from("-o"), String::from("ServerAliveInterval=60")]);
            }
            // SEGURIDAD: `--` = "aquí se acaban las opciones". Lo que venga
            // detrás es el destino aunque empiece por `-`: así un servidor o
            // usuario como `-oProxyCommand=…` (p. ej. en un perfiles.toml
            // editado a mano o ajeno) no se convierte en una opción de ssh.
            // El lanzador ya los rechaza (`perfil::dato_ssh_valido`); esto
            // es la segunda barrera, por si llegan por otro camino.
            comando.push(String::from("--"));
            comando.push(format!("{}@{}", perfil.usuario, perfil.host));
            abrir_pestana(app, &perfil.nombre, comando, false, None)
        }
    };

    if perfil.mantener_sesion {
        mantener::activar(&terminal, silencio);
    }

    // `mut`: esta lista sí la vamos a modificar (con push).
    let mut pasos = Vec::new();

    // 1. La contraseña de ssh (opcional: si entras con clave SSH no se pide).
    //    `&clave` = miramos dentro del Option sin quedárnoslo.
    //    En un perfil local no: ahí la contraseña solo se usa si el guion
    //    pone `{clave}` donde toca.
    if let (Some(clave), false) = (&clave, perfil.es_local()) {
        pasos.push(Paso {
            espera: vec![String::from("password:"), String::from("contraseña:")],
            escribe: clave.clone(),
            plazo: Duration::from_secs(30),
            opcional: true,
        });
    }

    // 2. El guion del perfil. 60 s por paso: da tiempo a teclear el OTP.
    //
    // Secretos que se pueden usar en "escribe": (marcador, valor si lo hay).
    // {root} sale de la entrada de KeePass del perfil, si la bóveda está abierta.
    // `as_deref()` convierte Option<String> en Option<&str>.
    let secretos = [
        ("{clave}", clave.clone()),
        (
            "{root}",
            perfil
                .keepass
                .as_deref()
                .and_then(boveda::clave_root)
                .map(|clave_root| {
                    if perfil.cambios_root {
                        boveda::aplicar_cambios(&clave_root)
                    } else {
                        clave_root
                    }
                }),
        ),
    ];
    for (numero, paso) in perfil.guion.iter().enumerate() {
        let mut escribe = paso.escribe.clone();
        let mut falta = None;
        for (marcador, valor) in &secretos {
            if escribe.contains(marcador) {
                match valor {
                    Some(valor) => escribe = escribe.replace(marcador, valor),
                    None => falta = Some(*marcador),
                }
            }
        }
        if let Some(marcador) = falta {
            // Sin ese secreto no podemos seguir: avisamos en la propia
            // terminal y cortamos el guion aquí (`break` sale del for).
            let aviso = format!(
                "[RetroTerm] El paso {} del guion usa {marcador}, pero no está \
                 disponible: el guion se detiene ahí.\r\n",
                numero + 1
            );
            terminal.feed(aviso.as_bytes());
            break;
        }
        pasos.push(Paso {
            // "Password:|Contraseña:" -> vale cualquiera de las dos.
            espera: paso
                .espera
                .split('|')
                .map(|texto| texto.trim().to_string())
                .filter(|texto| !texto.is_empty())
                .collect(),
            escribe,
            plazo: Duration::from_secs(60),
            opcional: false,
        });
    }

    if !pasos.is_empty() {
        autorespuesta::vigilar(&terminal, pasos);
    }
}

/// Crea una pestaña nueva, aplica el tema, lanza `comando` dentro y
/// devuelve la terminal y su "silencio" del aviso de actividad (una TUPLA:
/// dos valores a la vez; quien llama los separa con `let (a, b) = ...`).
///
/// `comando: Vec<String>` se recibe "por valor" (sin &): la función pasa a
/// ser su dueña. Quien la llama ya no puede usar ese Vec después.
///
/// `orden_inicial`: texto que se teclea en la terminal en cuanto arranca el
/// proceso (con su Intro), como si lo escribieras tú. Lo usan los perfiles
/// locales para lanzar su comando dentro de tu shell.
fn abrir_pestana(
    app: &gtk::Application,
    titulo: &str,
    comando: Vec<String>,
    es_local: bool,
    orden_inicial: Option<String>,
) -> (Terminal, actividad::Silencio) {
    let terminal = Terminal::new();
    terminal.set_scrollback_lines(10_000);
    terminal.set_mouse_autohide(true);
    tema::aplicar(&terminal, &ajustes::actuales());

    // La página de la pestaña no es la terminal a secas: es un Overlay con la
    // terminal debajo y la capa de efectos CRT encima.
    let pagina = crt::envolver(&terminal);

    let cuaderno = cuaderno_de_pestanas(app);
    let etiqueta = etiqueta_pestana(titulo, &terminal);
    let numero = cuaderno.append_page(&pagina, Some(&etiqueta));
    // El "texto de menú" de la pestaña: lo usamos para el título de la ventana.
    cuaderno.set_menu_label_text(&pagina, titulo);
    cuaderno.set_tab_reorderable(&pagina, true); // se pueden arrastrar
    cuaderno.set_current_page(Some(numero));
    poner_titulo(&cuaderno, &pagina);

    // Aviso de "hay cosas por leer" si esta pestaña recibe texto sin estar a la vista.
    let silencio = actividad::vigilar(&terminal, &pagina, &etiqueta);

    // spawn_async quiere `&[&str]` (textos prestados), y tenemos `Vec<String>`.
    // `.iter()` recorre, `.map(...)` transforma cada uno, `.collect()` junta.
    let argv: Vec<&str> = comando.iter().map(|s| s.as_str()).collect();
    // Una copia de la terminal para el closure de "ya arrancó" (se llama una
    // sola vez y luego se suelta, así que no forma un ciclo).
    let terminal_inicio = terminal.clone();

    terminal.spawn_async(
        PtyFlags::DEFAULT,
        None,
        &argv,
        &[],
        glib::SpawnFlags::SEARCH_PATH, // busca "ssh" en el PATH, como haría la shell
        || {},
        -1,
        None::<&gio::Cancellable>,
        // `move`: el closure se queda con `terminal_inicio` y `orden_inicial`.
        move |resultado| match resultado {
            Err(error) => eprintln!("No se pudo arrancar el comando: {error}"),
            Ok(_pid) => {
                if let Some(orden) = orden_inicial {
                    // `feed_child` = enviar texto al proceso como si se
                    // tecleara. Si la shell aún está arrancando, el sistema lo
                    // guarda y lo lee en cuanto esté lista. "\n" = Intro.
                    terminal_inicio.feed_child(format!("{orden}\n").as_bytes());
                }
            }
        },
    );

    terminal.connect_child_exited(move |terminal, estado| al_terminar(terminal, estado, es_local));

    instalar_menu_contextual(&terminal);
    instalar_copiar_al_seleccionar(&terminal);
    instalar_vigilancia_pegado(&terminal);

    terminal.grab_focus();
    (terminal, silencio) // sin `;`: este es el valor que devuelve la función
}

/// Qué hacer cuando acaba el proceso de una pestaña.
fn al_terminar(terminal: &Terminal, estado: i32, es_local: bool) {
    // `estado` es el "wait status" de Unix: un número que empaqueta dos cosas.
    // - Los 7 bits bajos: la señal que mató al proceso (0 = salió normalmente).
    // - Los 8 bits siguientes: el código de salida.
    // `&` es el Y binario y `>>` desplaza bits a la derecha.
    let salio_normalmente = (estado & 0x7f) == 0;
    let codigo = (estado >> 8) & 0xff;

    // ssh devuelve 255 cuando falla él (no el último comando remoto).
    let fallo_ssh = !es_local && (!salio_normalmente || codigo == 255);

    if fallo_ssh {
        // b"..." = cadena de BYTES (no de texto), que es lo que espera feed().
        // \r\n = retorno de carro + salto de línea, como en un teletipo.
        // (Incluye contraseña rechazada varias veces y Ctrl+C: ssh muere por
        // la señal SIGINT, así que no "sale normalmente".)
        terminal.feed(b"\r\n*** CONEXION FALLIDA - pulsa una tecla para continuar ***\r\n");
        // "commit" se emite cuando escribes algo en la terminal.
        // `true`: si era la última pestaña, se vuelve a la ventana de
        // conexiones en vez de salir del programa.
        terminal.connect_commit(|terminal, _texto, _longitud| cerrar_mas_tarde(terminal, true));
    } else {
        cerrar_mas_tarde(terminal, false);
    }
}

/// Cierra la pestaña "en cuanto GTK esté libre".
///
/// Estamos dentro de una señal de la propia terminal; destruirla justo ahora
/// es arriesgado. `idle_add_local_once` apunta la tarea para cuando el bucle
/// de eventos no tenga nada más que hacer (unos milisegundos después).
fn cerrar_mas_tarde(terminal: &Terminal, volver_al_inicio: bool) {
    let terminal = terminal.clone();
    glib::idle_add_local_once(move || cerrar_pestana(&terminal, volver_al_inicio));
}

/// Quita la pestaña de una terminal. Si era la última, cierra la ventana y:
/// - con `volver_al_inicio = true` (conexión fallida), abre la ventana de
///   conexiones para intentarlo otra vez;
/// - si no, sale del programa.
fn cerrar_pestana(terminal: &Terminal, volver_al_inicio: bool) {
    // Subimos por el árbol de widgets hasta encontrar el Notebook.
    let Some(cuaderno) = terminal
        .ancestor(gtk::Notebook::static_type())
        .and_downcast::<gtk::Notebook>()
    else {
        return; // ya no estaba en ninguna ventana
    };

    // La página: el Overlay de crt::envolver que contiene a la terminal.
    if let Some(pagina) = crt::pagina_de(terminal) {
        if let Some(numero) = cuaderno.page_num(&pagina) {
            cuaderno.remove_page(Some(numero));
        }
    }

    if cuaderno.n_pages() == 0 {
        if let Some(ventana) = cuaderno.root().and_downcast::<gtk::Window>() {
            let app = ventana.application();
            if volver_al_inicio {
                // Primero abrimos la de conexiones y DESPUÉS cerramos esta:
                // si la aplicación se quedara un instante sin ninguna ventana,
                // GTK la daría por terminada.
                if let Some(app) = &app {
                    lanzador::mostrar(app);
                }
                ventana.close();
            } else {
                ventana.close();
                // quit() termina app.run() en main.rs aunque quede otra ventana
                // abierta (la de conexiones): así acaba también `cargo run`.
                if let Some(app) = app {
                    app.quit();
                }
            }
        }
    }
}

/// La pestaña: el título y un botón ✕ para cerrarla.
fn etiqueta_pestana(titulo: &str, terminal: &Terminal) -> gtk::Box {
    let caja = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    caja.append(&gtk::Label::new(Some(titulo)));

    let boton_cerrar = gtk::Button::from_icon_name("window-close-symbolic");
    boton_cerrar.set_has_frame(false);
    boton_cerrar.set_tooltip_text(Some("Cerrar pestaña (Ctrl+Mayús+W)"));
    let terminal = terminal.clone();
    boton_cerrar.connect_clicked(move |_| cerrar_pestana(&terminal, false));
    caja.append(&boton_cerrar);

    caja
}

/// El Notebook (el widget de pestañas) de la ventana de terminales.
/// Si la ventana no existe todavía, la crea.
fn cuaderno_de_pestanas(app: &gtk::Application) -> gtk::Notebook {
    // Buscamos entre las ventanas de la aplicación una cuyo contenido sea un
    // Notebook. `and_downcast` intenta ver un widget genérico como un tipo
    // concreto: da `Some` si lo es y `None` si no.
    for ventana in app.windows() {
        if let Some(cuaderno) = cuaderno_de(&ventana) {
            ventana.present();
            return cuaderno;
        }
    }
    crear_ventana(app)
}

fn crear_ventana(app: &gtk::Application) -> gtk::Notebook {
    let cuaderno = gtk::Notebook::builder()
        .scrollable(true) // con muchas pestañas, aparecen flechas
        .show_border(false)
        .build();

    // Botón "+" a la derecha de las pestañas: abre la ventana de conexiones.
    let boton_nueva = gtk::Button::from_icon_name("tab-new-symbolic");
    boton_nueva.set_has_frame(false);
    boton_nueva.set_tooltip_text(Some("Nueva conexión (Ctrl+Mayús+T)"));
    {
        let app = app.clone();
        boton_nueva.connect_clicked(move |_| lanzador::mostrar(&app));
    }
    // Botón ⚙: la ventana de ajustes.
    let boton_ajustes = gtk::Button::from_icon_name("emblem-system-symbolic");
    boton_ajustes.set_has_frame(false);
    boton_ajustes.set_tooltip_text(Some("Ajustes (Ctrl+,)"));
    {
        let app = app.clone();
        boton_ajustes.connect_clicked(move |_| preferencias::mostrar(&app));
    }

    // Botón candado: bloquear RetroTerm (salvapantallas + contraseña de sesión).
    let boton_bloqueo = gtk::Button::from_icon_name("system-lock-screen-symbolic");
    boton_bloqueo.set_has_frame(false);
    boton_bloqueo.set_tooltip_text(Some("Bloquear (Ctrl+Mayús+L)"));
    boton_bloqueo.add_css_class("boton-bloqueo"); // para encontrarlo en aplicar_ajustes
    boton_bloqueo.set_visible(ajustes::actuales().bloqueo_activado);
    boton_bloqueo.connect_clicked(|boton| {
        if let Some(ventana) = boton.root().and_downcast::<gtk::Window>() {
            bloqueo::bloquear(&ventana);
        }
    });

    // Botón 📝: mostrar u ocultar el bloc de notas (bloc.rs). Lanza la
    // acción "win.bloc", la misma que el atajo Ctrl+Mayús+B.
    let boton_bloc = gtk::Button::from_icon_name("document-edit-symbolic");
    boton_bloc.set_has_frame(false);
    boton_bloc.set_tooltip_text(Some("Bloc de notas (Ctrl+Mayús+B)"));
    boton_bloc.set_action_name(Some("win.bloc"));

    // Los botones juntos, a la derecha de las pestañas.
    let botones = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    botones.append(&boton_nueva);
    botones.append(&boton_bloc);
    botones.append(&boton_ajustes);
    botones.append(&boton_bloqueo);
    cuaderno.set_action_widget(&botones, gtk::PackType::End);

    // Al cambiar de pestaña, el título de la ventana sigue a la pestaña activa.
    // Y la pestaña a la que cambias deja de avisar de actividad.
    cuaderno.connect_switch_page(|cuaderno, pagina, _numero| {
        poner_titulo(cuaderno, pagina);
        if let Some(etiqueta) = cuaderno.tab_label(pagina) {
            actividad::apagar(&etiqueta);
        }
    });

    // Doble clic en el HUECO VACÍO de la barra de pestañas: nueva conexión.
    // (El gesto escucha en todo el Notebook; `es_hueco_de_pestanas` decide si
    // el clic fue de verdad en el hueco y no en una pestaña o en la terminal.)
    let doble_clic = gtk::GestureClick::new();
    doble_clic.set_button(1); // botón izquierdo
    {
        let app = app.clone();
        // Referencia DÉBIL al cuaderno: el gesto vive dentro del propio
        // cuaderno; una referencia normal crearía un ciclo que nunca se libera.
        let cuaderno_debil = cuaderno.downgrade();
        doble_clic.connect_pressed(move |_gesto, pulsaciones, x, y| {
            if pulsaciones != 2 {
                return; // solo nos interesa el segundo clic de un doble clic
            }
            let Some(cuaderno) = cuaderno_debil.upgrade() else {
                return;
            };
            if es_hueco_de_pestanas(&cuaderno, x, y) {
                lanzador::mostrar(&app);
            }
        });
    }
    cuaderno.add_controller(doble_clic);

    // Debajo de todo, un `Paned`: dos zonas con un divisor que se arrastra.
    // A la izquierda, el cuaderno de pestañas; a la derecha, el bloc (oculto
    // al principio; con él oculto, el cuaderno ocupa todo el ancho).
    // `resize`: quién crece al agrandar la ventana (solo las terminales: el
    // bloc conserva su ancho). `shrink: false`: ninguno se puede encoger
    // por debajo de su tamaño mínimo.
    let contenido = gtk::Paned::new(gtk::Orientation::Horizontal);
    contenido.set_start_child(Some(&cuaderno));
    contenido.set_end_child(Some(&bloc::crear()));
    contenido.set_resize_start_child(true);
    contenido.set_shrink_start_child(false);
    contenido.set_resize_end_child(false);
    contenido.set_shrink_end_child(false);

    // La raíz de la ventana es un Overlay: el contenido debajo y, encima,
    // la capa de bloqueo (oculta hasta que se bloquea; tapa también el bloc).
    let raiz = gtk::Overlay::new();
    raiz.set_child(Some(&contenido));
    raiz.add_overlay(&bloqueo::crear_capa());

    let ventana = gtk::ApplicationWindow::builder()
        .application(app)
        .title("RetroTerm")
        .default_width(960)
        .default_height(600)
        .child(&raiz)
        .build();
    bloqueo::instalar(&ventana);
    ventana.add_css_class("terminales"); // estilos de pestañas en tema::css()

    // ACCIONES: órdenes con nombre ("win.nueva") a las que se asignan atajos
    // de teclado. Es la forma estándar de GTK de hacer atajos.
    let nueva = gio::SimpleAction::new("nueva", None);
    {
        let app = app.clone();
        nueva.connect_activate(move |_, _| {
            if !bloqueo::esta_bloqueado() {
                lanzador::mostrar(&app);
            }
        });
    }
    ventana.add_action(&nueva);

    let cerrar = gio::SimpleAction::new("cerrar-pestana", None);
    {
        let cuaderno = cuaderno.clone();
        cerrar.connect_activate(move |_, _| {
            if bloqueo::esta_bloqueado() {
                return; // bloqueado: ninguna acción sobre las terminales
            }
            if let Some(terminal) = terminal_activa(&cuaderno) {
                cerrar_pestana(&terminal, false);
            }
        });
    }
    ventana.add_action(&cerrar);

    // Copiar y pegar actúan sobre la terminal de la pestaña activa.
    let copiar = gio::SimpleAction::new("copiar", None);
    {
        let cuaderno = cuaderno.clone();
        copiar.connect_activate(move |_, _| {
            if bloqueo::esta_bloqueado() {
                return; // bloqueado: ninguna acción sobre las terminales
            }
            // Escribiendo en el bloc: copiar de él, no de la terminal.
            if let Some(vista) = vista_bloc_con_foco(&cuaderno) {
                vista.emit_copy_clipboard();
                return;
            }
            if let Some(terminal) = terminal_activa(&cuaderno) {
                // Solo si hay algo seleccionado; si no, el portapapeles no se toca.
                if terminal.has_selection() {
                    terminal.copy_clipboard_format(Format::Text);
                }
            }
        });
    }
    ventana.add_action(&copiar);

    let pegar = gio::SimpleAction::new("pegar", None);
    {
        let cuaderno = cuaderno.clone();
        pegar.connect_activate(move |_, _| {
            if bloqueo::esta_bloqueado() {
                return; // bloqueado: ninguna acción sobre las terminales
            }
            // Escribiendo en el bloc: pegar en él (sin avisos: no ejecuta nada).
            if let Some(vista) = vista_bloc_con_foco(&cuaderno) {
                vista.emit_paste_clipboard();
                return;
            }
            if let Some(terminal) = terminal_activa(&cuaderno) {
                // Pega como siempre, salvo que haya que avisar (pegado.rs).
                pegado::pegar(&terminal, Origen::Portapapeles);
            }
        });
    }
    ventana.add_action(&pegar);

    let accion_ajustes = gio::SimpleAction::new("preferencias", None);
    {
        let app = app.clone();
        accion_ajustes.connect_activate(move |_, _| {
            if !bloqueo::esta_bloqueado() {
                preferencias::mostrar(&app);
            }
        });
    }
    ventana.add_action(&accion_ajustes);

    let accion_acerca = gio::SimpleAction::new("acerca", None);
    {
        // Ojo: el closure no debe guardar la ventana (crearía un ciclo de
        // referencias); la pedimos al cuaderno en el momento del clic.
        let cuaderno = cuaderno.clone();
        accion_acerca.connect_activate(move |_, _| {
            if bloqueo::esta_bloqueado() {
                return; // bloqueado: ninguna acción sobre las terminales
            }
            let ventana = cuaderno.root().and_downcast::<gtk::Window>();
            acerca::mostrar(ventana.as_ref());
        });
    }
    ventana.add_action(&accion_acerca);

    let accion_bloquear = gio::SimpleAction::new("bloquear", None);
    {
        let cuaderno = cuaderno.clone();
        accion_bloquear.connect_activate(move |_, _| {
            if let Some(ventana) = cuaderno.root().and_downcast::<gtk::Window>() {
                bloqueo::bloquear(&ventana);
            }
        });
    }
    ventana.add_action(&accion_bloquear);

    // Mostrar u ocultar el bloc de notas. En los dos casos el foco queda en
    // la terminal activa: al abrirlo, porque lo normal es seguir trabajando
    // en la consola (para escribir en el bloc, un clic en él); al cerrarlo,
    // porque si no se quedaría "en el aire". (Pulsar el botón 📝 también le
    // quita el foco a la terminal: así se lo devolvemos.)
    let accion_bloc = gio::SimpleAction::new("bloc", None);
    {
        // Débil: la acción vive en la ventana, que contiene al Paned.
        let contenido = contenido.downgrade();
        let cuaderno = cuaderno.downgrade();
        accion_bloc.connect_activate(move |_, _| {
            if bloqueo::esta_bloqueado() {
                return;
            }
            let (Some(contenido), Some(cuaderno)) = (contenido.upgrade(), cuaderno.upgrade())
            else {
                return;
            };
            bloc::alternar(&contenido);
            if let Some(terminal) = terminal_activa(&cuaderno) {
                terminal.grab_focus();
            }
        });
    }
    ventana.add_action(&accion_bloc);

    // ATAJOS. Usamos un ShortcutController en fase de CAPTURA: los eventos de
    // teclado recorren el árbol de widgets de la ventana hacia abajo
    // ("captura") y luego hacia arriba ("burbuja"). En captura, la ventana ve
    // la tecla ANTES que la terminal, que si no se la quedaría para el
    // programa que se esté ejecutando.
    let atajos = gtk::ShortcutController::new();
    atajos.set_propagation_phase(gtk::PropagationPhase::Capture);
    // Un array de tuplas (tecla, acción), recorrido con `for`.
    for (tecla, accion) in [
        ("<Control><Shift>t", "win.nueva"),
        ("<Control><Shift>w", "win.cerrar-pestana"),
        ("<Control><Shift>c", "win.copiar"),
        ("<Control><Shift>v", "win.pegar"),
        ("<Shift>Insert", "win.pegar"),
        ("<Control>comma", "win.preferencias"),
        ("<Control><Shift>l", "win.bloquear"),
        ("<Control><Shift>b", "win.bloc"),
    ] {
        atajos.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string(tecla),
            Some(gtk::NamedAction::new(accion)),
        ));
    }
    ventana.add_controller(atajos);

    ventana.present();
    cuaderno
}

/// ¿El punto (x, y) del cuaderno cae en el hueco vacío de la barra de pestañas?
///
/// `pick` devuelve el widget MÁS PROFUNDO que hay en ese punto. Subimos desde
/// él hasta el cuaderno mirando por dónde pasamos:
/// - una pestaña ("tab"), un botón (✕, +, ⚙) o una página (el Overlay con la
///   terminal) -> NO es el hueco;
/// - si pasamos por la cabecera ("header" / "tabs") y nada de lo anterior -> SÍ.
///
/// "tab", "header" y "tabs" son los NOMBRES CSS de las piezas internas del
/// Notebook de GTK: los mismos que usamos en tema::css() para darles estilo.
fn es_hueco_de_pestanas(cuaderno: &gtk::Notebook, x: f64, y: f64) -> bool {
    let Some(mut widget) = cuaderno.pick(x, y, gtk::PickFlags::DEFAULT) else {
        return false;
    };
    let raiz: &gtk::Widget = cuaderno.upcast_ref();
    let mut en_cabecera = false;

    // `loop` = bucle sin condición; se sale con `return` (o `break`).
    loop {
        if &widget == raiz {
            return en_cabecera;
        }
        let nombre = widget.css_name();
        if nombre.as_str() == "tab" || widget.is::<gtk::Button>() || widget.is::<gtk::Overlay>() {
            return false;
        }
        if nombre.as_str() == "header" || nombre.as_str() == "tabs" {
            en_cabecera = true;
        }
        widget = match widget.parent() {
            Some(padre) => padre,
            None => return false,
        };
    }
}

/// Pone en la ventana el título de la pestaña `pagina`.
fn poner_titulo(cuaderno: &gtk::Notebook, pagina: &impl IsA<gtk::Widget>) {
    if bloqueo::esta_bloqueado() {
        return; // bloqueado: el título no debe decir a qué servidor estás conectado
    }
    let titulo = cuaderno.menu_label_text(pagina);
    if let (Some(titulo), Some(ventana)) = (titulo, cuaderno.root().and_downcast::<gtk::Window>()) {
        ventana.set_title(Some(&format!("RetroTerm — {titulo}")));
    }
}

/// La terminal de la pestaña que se está viendo.
fn terminal_activa(cuaderno: &gtk::Notebook) -> Option<Terminal> {
    // Página activa -> su terminal (ver crt::terminal_de).
    // `and_then` encadena pasos que devuelven Option: si uno da None, el
    // resultado entero es None.
    cuaderno
        .nth_page(cuaderno.current_page())
        .and_then(|pagina| crt::terminal_de(&pagina))
}

/// Aviso al pegar en root: intercepta las formas de pegar que VTE gestiona
/// por su cuenta (Mayús+Insert y el botón central), además de Ctrl+Mayús+V.
///
/// Solo intervienen si hay que vigilar (opción activada y prompt de root);
/// si no, dejan pasar el evento y todo funciona como siempre.
fn instalar_vigilancia_pegado(terminal: &Terminal) {
    // Teclado en fase de CAPTURA: lo vemos antes que la propia terminal.
    let teclas = gtk::EventControllerKey::new();
    teclas.set_propagation_phase(gtk::PropagationPhase::Capture);
    teclas.connect_key_pressed(|controlador, tecla, _codigo, modificadores| {
        // `widget()`: el widget al que está enganchado el controlador.
        let Some(terminal) = controlador.widget().and_downcast::<Terminal>() else {
            return glib::Propagation::Proceed;
        };
        let control = modificadores.contains(gdk::ModifierType::CONTROL_MASK);
        let mayus = modificadores.contains(gdk::ModifierType::SHIFT_MASK);
        // Con Mayús pulsada, la tecla llega como `V` mayúscula.
        let es_pegar = (control && mayus && (tecla == gdk::Key::V || tecla == gdk::Key::v))
            || (mayus && !control && (tecla == gdk::Key::Insert || tecla == gdk::Key::KP_Insert));
        if es_pegar && pegado::hay_que_vigilar(&terminal) {
            pegado::pegar(&terminal, Origen::Portapapeles);
            return glib::Propagation::Stop; // nadie más lo pega
        }
        glib::Propagation::Proceed
    });
    terminal.add_controller(teclas);

    // Botón central (2), también en captura.
    let clic = gtk::GestureClick::new();
    clic.set_button(2);
    clic.set_propagation_phase(gtk::PropagationPhase::Capture);
    clic.connect_pressed(|gesto, _pulsaciones, _x, _y| {
        let Some(terminal) = gesto.widget().and_downcast::<Terminal>() else {
            return;
        };
        if pegado::hay_que_vigilar(&terminal) {
            // "Reclamar" el gesto: el clic es nuestro y VTE no lo recibe.
            gesto.set_state(gtk::EventSequenceState::Claimed);
            pegado::pegar(&terminal, Origen::Primaria);
        }
    });
    terminal.add_controller(clic);
}

/// Tras esta pausa sin que cambie la selección, se copia al portapapeles.
const PAUSA_COPIAR: Duration = Duration::from_millis(300);

/// "Copiar al seleccionar": al seleccionar texto, se copia también al
/// portapapeles normal (el de Ctrl+V en otros programas), si está activado
/// en Ajustes.
///
/// La señal "selection-changed" salta MUCHAS veces mientras arrastras el
/// ratón. Para no copiar cada trozo a medias (y llenar el historial de
/// Klipper), esperamos a que la selección deje de cambiar `PAUSA_COPIAR`:
/// cada cambio cancela el temporizador anterior y pone uno nuevo.
fn instalar_copiar_al_seleccionar(terminal: &Terminal) {
    // El temporizador pendiente, si hay uno. `Rc<RefCell<...>>` porque lo
    // comparten el manejador de la señal y el propio temporizador, y los dos
    // lo cambian. `glib::SourceId` identifica un temporizador de GLib.
    let pendiente: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

    terminal.connect_selection_changed(move |terminal| {
        // Cancelar el temporizador anterior, si aún no se había ejecutado.
        // Primero lo sacamos (`take()` deja `None` en su sitio) y soltamos el
        // préstamo; después lo cancelamos con `remove()`.
        let anterior = pendiente.borrow_mut().take();
        if let Some(temporizador) = anterior {
            temporizador.remove();
        }
        if !ajustes::actuales().copiar_al_seleccionar || !terminal.has_selection() {
            return;
        }
        // Referencia débil: si la pestaña se cierra antes de la pausa, el
        // temporizador no la mantiene viva (`upgrade()` dará `None`).
        let debil = terminal.downgrade();
        let pendiente_dentro = pendiente.clone();
        let temporizador = glib::timeout_add_local_once(PAUSA_COPIAR, move || {
            // Ya se está ejecutando: lo olvidamos SIN `remove()` (cancelar un
            // temporizador que ya terminó da un aviso de GLib).
            pendiente_dentro.borrow_mut().take();
            if let Some(terminal) = debil.upgrade() {
                if terminal.has_selection() {
                    terminal.copy_clipboard_format(Format::Text);
                }
            }
        });
        *pendiente.borrow_mut() = Some(temporizador);
    });
}

/// Clic derecho sobre la terminal: un pequeño menú con Copiar y Pegar.
fn instalar_menu_contextual(terminal: &Terminal) {
    // El modelo del menú: textos y el nombre de la acción que ejecuta cada uno.
    // Las acciones "win.*" son las que añadimos a la ventana en crear_ventana.
    let menu = gio::Menu::new();
    menu.append(Some("Copiar          Ctrl+Mayús+C"), Some("win.copiar"));
    menu.append(Some("Pegar           Ctrl+Mayús+V"), Some("win.pegar"));
    menu.append(Some("Ajustes…        Ctrl+,"), Some("win.preferencias"));
    menu.append(Some("Bloquear          Ctrl+Mayús+L"), Some("win.bloquear"));
    menu.append(Some("Acerca de RetroTerm"), Some("win.acerca"));

    // GestureClick = "escucha clics de ratón"; set_button(3) = botón derecho.
    let clic = gtk::GestureClick::new();
    clic.set_button(3);
    {
        let terminal = terminal.clone();
        clic.connect_pressed(move |_gesto, _pulsaciones, x, y| {
            let emergente = gtk::PopoverMenu::from_model(Some(&menu));
            emergente.set_parent(&terminal);
            emergente.set_has_arrow(false);
            // Que salga justo donde está el ratón (un rectángulo de 1×1).
            emergente.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            // Al cerrarse, lo "desenganchamos" de la terminal para que no se
            // vayan acumulando menús (y GTK no avise al cerrar la pestaña).
            emergente.connect_closed(|emergente| {
                let emergente = emergente.clone();
                glib::idle_add_local_once(move || emergente.unparent());
            });
            emergente.popup();
        });
    }
    terminal.add_controller(clic);
}

/// Vuelve a aplicar los ajustes actuales a TODAS las pestañas abiertas
/// (fuente, colores y capa CRT). La llama la ventana de ajustes.
pub fn aplicar_ajustes(app: &gtk::Application) {
    let ajustes = ajustes::actuales();
    for ventana in app.windows() {
        let Some(cuaderno) = cuaderno_de(&ventana) else {
            continue; // `continue` = pasar a la siguiente vuelta del `for`
        };
        mostrar_boton_bloqueo(&cuaderno, ajustes.bloqueo_activado);
        // `0..n` recorre los números de página.
        for numero in 0..cuaderno.n_pages() {
            let Some(overlay) = cuaderno
                .nth_page(Some(numero))
                .and_downcast::<gtk::Overlay>()
            else {
                continue;
            };
            if let Some(terminal) = crt::terminal_de(overlay.upcast_ref()) {
                tema::aplicar(&terminal, &ajustes);
            }
            crt::repintar(&overlay);
        }
    }
}

/// El cuaderno de pestañas de una ventana de terminales, si lo es.
///
/// La ventana contiene un Overlay (raíz) y este, el Notebook. Para cualquier
/// otra ventana (conexiones, ajustes...), devuelve None.
pub fn cuaderno_de(ventana: &impl IsA<gtk::Window>) -> Option<gtk::Notebook> {
    contenido_de(ventana)?
        .start_child()
        .and_downcast::<gtk::Notebook>()
}

/// El `Paned` de la ventana de terminales: [cuaderno | bloc de notas].
/// (Raíz Overlay → Paned; ver la estructura de widgets en CLAUDE.md.)
pub fn contenido_de(ventana: &impl IsA<gtk::Window>) -> Option<gtk::Paned> {
    ventana
        .child()
        .and_downcast::<gtk::Overlay>()?
        .child()
        .and_downcast::<gtk::Paned>()
}

/// El área de texto del bloc, si tiene el foco (ver `bloc::vista_con_foco`).
fn vista_bloc_con_foco(cuaderno: &gtk::Notebook) -> Option<gtk::TextView> {
    let ventana = cuaderno.root().and_downcast::<gtk::Window>()?;
    bloc::vista_con_foco(&ventana)
}

/// Tras desbloquear: título de la pestaña activa y foco en su terminal.
pub fn tras_desbloquear(ventana: &gtk::Window) {
    let Some(cuaderno) = cuaderno_de(ventana) else {
        return;
    };
    if let Some(pagina) = cuaderno.nth_page(cuaderno.current_page()) {
        poner_titulo(&cuaderno, &pagina);
    }
    if let Some(terminal) = terminal_activa(&cuaderno) {
        terminal.grab_focus();
    }
}

/// Muestra u oculta el botón candado (según Ajustes → Bloqueo).
fn mostrar_boton_bloqueo(cuaderno: &gtk::Notebook, visible: bool) {
    let Some(botones) = cuaderno.action_widget(gtk::PackType::End) else {
        return;
    };
    let mut hijo = botones.first_child();
    while let Some(widget) = hijo {
        if widget.has_css_class("boton-bloqueo") {
            widget.set_visible(visible);
        }
        hijo = widget.next_sibling();
    }
}
