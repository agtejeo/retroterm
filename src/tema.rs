// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Temas retro: colores de fósforo, cómo se aplican a la terminal y el CSS.
//!
//! Este fichero es un MÓDULO. En main.rs lo "enchufamos" con `mod tema;`
//! y desde allí usamos lo que aquí esté marcado como `pub` (público).
//! Lo que no lleva `pub` solo es visible dentro de este fichero.

// `use` trae nombres de otros módulos/crates para no escribir la ruta completa.
use vte4::gtk::{self, gdk, pango};
use vte4::prelude::*; // el "preludio" añade los métodos (set_font, etc.) a Terminal
use vte4::{CursorBlinkMode, CursorShape, Terminal};

use crate::ajustes::{self, Ajustes};
use crate::paleta;

/// Los colores de un fósforo. Un `struct` agrupa datos con nombre.
///
/// `&'static str` = un texto que vive durante todo el programa
/// (los textos escritos entre comillas en el código lo son).
pub struct Tema {
    pub nombre: &'static str,
    pub texto: &'static str, // colores en hexadecimal, como en CSS
    pub fondo: &'static str,
    pub cursor: &'static str,
}

// `const` = valor fijo, conocido al compilar. Por convención, en MAYÚSCULAS.
pub const FOSFORO_VERDE: Tema = Tema {
    nombre: "Verde (P1)",
    texto: "#33ff66",
    fondo: "#0a140c",
    cursor: "#66ff99",
};

pub const FOSFORO_AMBAR: Tema = Tema {
    nombre: "Ámbar (P3)",
    texto: "#ffb000",
    fondo: "#140d00",
    cursor: "#ffcc44",
};

pub const FOSFORO_BLANCO: Tema = Tema {
    nombre: "Blanco (P4)",
    texto: "#d8e4ec",
    fondo: "#0c0e10",
    cursor: "#ffffff",
};

/// Aplica fuente, colores y cursor a una terminal según los ajustes.
///
/// `&Terminal`, `&Ajustes` -> tomamos prestados los dos (no los copiamos).
/// Este sistema de préstamos (`&`) es LA idea central de Rust: el
/// compilador comprueba que nadie usa algo que ya no existe.
pub fn aplicar(terminal: &Terminal, ajustes: &Ajustes) {
    let tema = ajustes.tema();

    // "VT323, Monospace 20": la fuente elegida y, si no existiera, cualquier
    // monoespaciada; después, el tamaño.
    let descripcion = format!("{}, Monospace {}", ajustes.fuente, ajustes.tamano);
    let fuente = pango::FontDescription::from_string(&descripcion);
    // `Some(x)` significa "hay un valor". La alternativa es `None`.
    // Rust no tiene `null`: lo opcional se expresa con `Option`.
    terminal.set_font(Some(&fuente));

    let texto = color(tema.texto);
    let fondo = color(tema.fondo);
    // Paleta vacía = la paleta de colores normal de VTE (colores originales).
    let colores = if ajustes.monocromo {
        paleta::monocroma(&fondo, &texto)
    } else {
        Vec::new()
    };
    // set_colors va PRIMERO: fija texto, fondo y paleta de una vez, y el
    // resto de colores (negrita, cursor) se ajustan después.
    // set_colors pide `&[&RGBA]` (una lista de REFERENCIAS a colores) y
    // tenemos `Vec<RGBA>` (los colores en sí). `.iter()` recorre el Vec
    // dando `&RGBA`, y `.collect()` los junta en un Vec de referencias.
    let referencias: Vec<&gdk::RGBA> = colores.iter().collect();
    terminal.set_colors(Some(&texto), Some(&fondo), &referencias);
    if ajustes.monocromo {
        terminal.set_color_bold(Some(&color(tema.cursor)));
    } else {
        terminal.set_color_bold(None); // negrita en el color normal de cada texto
    }
    terminal.set_color_cursor(Some(&color(tema.cursor)));
    // Letra bajo el cursor en color de fondo: efecto "vídeo inverso" clásico.
    terminal.set_color_cursor_foreground(Some(&fondo));

    // El cursor: bloque macizo y parpadeante, como en un VT100.
    terminal.set_cursor_shape(CursorShape::Block);
    terminal.set_cursor_blink_mode(CursorBlinkMode::On);
    terminal.set_bold_is_bright(true);

    // La «campana»: cuando un programa envía el carácter BEL (p. ej. bash
    // al pulsar Retroceso sin nada que borrar), VTE pita. Lo decide el ajuste.
    terminal.set_audible_bell(ajustes.sonido);
}

// El "proveedor" de CSS de toda la aplicación. Lo guardamos para poder
// recargarlo cuando cambien los ajustes (ver `thread_local!` en ajustes.rs).
thread_local! {
    static PROVEEDOR_CSS: gtk::CssProvider = gtk::CssProvider::new();
}

/// Registra el CSS para todas las ventanas. Se llama una vez, al arrancar.
/// (Sí: GTK se estiliza con CSS, como una página web.)
pub fn instalar_estilos() {
    // `.expect(...)`: si no hay pantalla gráfica, paramos con este mensaje.
    let pantalla = gdk::Display::default().expect("No se encontró una pantalla gráfica");
    PROVEEDOR_CSS.with(|proveedor| {
        gtk::style_context_add_provider_for_display(
            &pantalla,
            proveedor,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    recargar_estilos();
}

/// Vuelve a generar el CSS con los ajustes actuales.
///
/// También aplica el ajuste «Sonido» a los campos de texto de GTK: pitan
/// (la «campana de error») al borrar sin nada que borrar, al mover el
/// cursor más allá del final… Es un ajuste global de GTK (`gtk-error-bell`)
/// que afecta a toda la aplicación. La campana de las TERMINALES va aparte
/// (`set_audible_bell` en `aplicar`).
pub fn recargar_estilos() {
    let ajustes = ajustes::actuales();
    let css = css(&ajustes);
    PROVEEDOR_CSS.with(|proveedor| proveedor.load_from_data(&css));
    // `if let Some(...)`: `Settings::default()` es un Option (podría no
    // haber pantalla); si no lo hay, no hacemos nada.
    if let Some(preferencias_gtk) = gtk::Settings::default() {
        preferencias_gtk.set_gtk_error_bell(ajustes.sonido);
    }
}

/// CSS de todas las ventanas con los colores y la fuente de los ajustes.
///
/// En `format!`, `{texto}` se sustituye por un valor; por eso las llaves
/// propias del CSS se escriben dobles: `{{` y `}}`.
fn css(ajustes: &Ajustes) -> String {
    let tema = ajustes.tema();
    format!(
        "
            window.retro {{ background-color: {fondo}; }}

            window.retro label,
            window.retro entry,
            window.retro button {{
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
                color: {texto};
            }}

            window.retro entry,
            window.retro button {{
                background: {fondo};
                background-image: none;
                border: 1px solid {texto};
                border-radius: 0;
                box-shadow: none;
                caret-color: {cursor};
            }}
            window.retro entry > text > placeholder {{ color: alpha({texto}, 0.4); }}

            window.retro button:hover {{ background: {texto}; }}
            window.retro button:hover label {{ color: {fondo}; }}

            window.retro popover contents {{
                background: {fondo};
                border: 1px solid {texto};
                border-radius: 0;
            }}

            window.retro scrolledwindow.guion {{ border: 1px solid {texto}; }}
            window.retro textview,
            window.retro textview text {{
                background: {fondo};
                color: {texto};
                caret-color: {cursor};
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
            }}
            window.retro .ayuda {{ font-size: 16px; opacity: 0.7; }}

            window.retro .titulo {{ font-size: 36px; }}

            /* Pestañas de la ventana de terminales */
            window.terminales notebook > header {{
                background: {fondo};
                border-bottom: 1px solid {texto};
                box-shadow: none;
            }}
            window.terminales notebook > header tab {{
                background: {fondo};
                border: 1px solid {texto};
                border-radius: 0;
                box-shadow: none;
                margin: 3px 2px;
                padding: 0 6px;
                min-height: 0;
            }}
            window.terminales notebook > header tab label {{
                font-family: \"{fuente}\", monospace;
                font-size: 18px;
                color: {texto};
            }}
            window.terminales notebook > header tab:checked {{ background: {texto}; }}

            /* Pestaña con «cosas por leer»: un barrido de luz del color del
               fósforo que la recorre de izquierda a derecha, en bucle.
               El degradado mide el triple que la pestaña; al mover su
               posición del 100 % al 0 %, la franja brillante la cruza. */
            @keyframes retroterm-barrido {{
                from {{ background-position: 100% 0; }}
                to   {{ background-position: 0% 0; }}
            }}
            window.terminales notebook > header tab box.actividad {{
                background-image: linear-gradient(to right,
                    transparent 0%, transparent 38%,
                    alpha({texto}, 0.55) 50%,
                    transparent 62%, transparent 100%);
                background-size: 300% 100%;
                background-repeat: no-repeat;
                animation: retroterm-barrido 1.6s linear infinite;
            }}
            window.terminales notebook > header tab box.actividad label {{
                font-weight: bold;
            }}
            window.terminales notebook > header tab:checked label,
            window.terminales notebook > header tab:checked button {{ color: {fondo}; }}

            /* Menú «Cambiar nombre…» de una pestaña (ventana_terminal.rs). Cuelga
               de la pestaña: sin estas reglas heredaría las de arriba (en la
               pestaña activa, letras del color del fondo: invisibles). Los
               selectores repiten «notebook > header tab» para pesar MÁS que
               esas reglas (especificidad de CSS). */
            window.terminales notebook > header tab popover.renombrar contents {{
                background: {fondo};
                border: 1px solid {texto};
                border-radius: 0;
            }}
            window.terminales notebook > header tab popover.renombrar button {{
                background: none;
                border-radius: 0;
                padding: 2px 8px;
            }}
            window.terminales notebook > header tab popover.renombrar button label {{
                font-family: \"{fuente}\", monospace;
                font-size: 18px;
                color: {texto};
            }}
            window.terminales notebook > header tab popover.renombrar button:hover {{ background: {texto}; }}
            window.terminales notebook > header tab popover.renombrar button:hover label {{ color: {fondo}; }}
            window.terminales notebook > header tab popover.renombrar entry {{
                background: {fondo};
                color: {texto};
                caret-color: {cursor};
                border: 1px solid {texto};
                border-radius: 0;
                box-shadow: none;
                font-family: \"{fuente}\", monospace;
                font-size: 18px;
            }}
            window.terminales notebook > header tab popover.renombrar entry selection {{
                background-color: {texto};
                color: {fondo};
            }}
            /* Margen interior de la terminal: que el texto no toque las
               esquinas redondeadas del efecto CRT. */
            window.terminales vte-terminal {{ padding: 18px 26px; }}

            /* Pantalla de bloqueo: la caja de la contraseña sobre el osciloscopio */
            window.terminales .caja-clave {{
                background: {fondo};
                border: 1px solid {texto};
                padding: 18px 24px;
            }}
            window.terminales .caja-clave label {{
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
                color: {texto};
            }}
            window.terminales .caja-clave .estado-clave {{ color: {cursor}; }}
            window.terminales .caja-clave entry {{
                background: {fondo};
                color: {texto};
                caret-color: {cursor};
                border: 1px solid {texto};
                border-radius: 0;
                box-shadow: none;
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
            }}

            /* Menú contextual (clic derecho) */
            window.terminales popover contents {{
                background: {fondo};
                border: 1px solid {texto};
                border-radius: 0;
            }}
            window.terminales popover modelbutton {{ border-radius: 0; }}
            window.terminales popover modelbutton label {{
                font-family: \"{fuente}\", monospace;
                font-size: 18px;
                color: {texto};
            }}
            window.terminales popover modelbutton:hover {{ background: {texto}; }}
            window.terminales popover modelbutton:hover label {{ color: {fondo}; }}

            window.terminales notebook button {{
                color: {texto};
                background: none;
                border: none;
                box-shadow: none;
                min-height: 0;
                min-width: 0;
                padding: 0 2px;
            }}
            window.retro .aviso {{ color: {cursor}; }}

            /* Ventana de ajustes: casillas y selector numérico */
            window.retro check {{
                background: {fondo};
                background-image: none;
                border: 1px solid {texto};
                border-radius: 0;
                box-shadow: none;
                color: {fondo};
            }}
            window.retro check:checked {{ background: {texto}; }}
            window.retro spinbutton {{
                background: {fondo};
                border: 1px solid {texto};
                border-radius: 0;
                box-shadow: none;
                color: {texto};
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
            }}
            window.retro spinbutton text,
            window.retro spinbutton button {{
                color: {texto};
                background: {fondo};
                border: none;
                box-shadow: none;
            }}
            window.retro spinbutton button:hover {{ background: {texto}; color: {fondo}; }}

            /* Botón del desplegable abierto o con foco: sin el azul del tema */
            window.retro dropdown > button {{
                background: {fondo};
                outline-color: {texto};
            }}
            window.retro selection {{ background-color: {texto}; color: {fondo}; }}

            /* Bloc de notas (bloc.rs), a la derecha de las terminales. Nítido:
               sin líneas CRT ni halo, para editar cómodo. */
            window.terminales paned > separator {{ background: {texto}; min-width: 2px; }}
            window.terminales .bloc {{ background: {fondo}; }}
            window.terminales .bloc .titulo-bloc {{
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
                color: {texto};
                padding: 6px 12px;
                border-bottom: 1px solid alpha({texto}, 0.5);
            }}
            window.terminales .bloc textview,
            window.terminales .bloc textview text {{
                background: {fondo};
                color: {texto};
                caret-color: {cursor};
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
            }}
            window.terminales .bloc textview text selection {{
                background-color: {texto};
                color: {fondo};
            }}

            /* Deslizador (intensidad de las líneas) */
            window.retro scale trough {{
                background: {fondo};
                border: 1px solid {texto};
                border-radius: 0;
                min-height: 6px;
            }}
            window.retro scale highlight {{ background: {texto}; border-radius: 0; }}
            /* El tema del sistema da al deslizador márgenes negativos que,
               al quitarle el borde, dejaban su tamaño en -2 píxeles (los
               avisos de la consola). Margen 0 y tamaño explícito lo evitan. */
            window.retro scale slider {{
                background: {texto};
                border: none;
                box-shadow: none;
                margin: 0;
                min-width: 14px;
                min-height: 14px;
            }}

            /* Barras de desplazamiento (cuadro del guion, listas, Ajustes...).
               Este era el origen real de los avisos «GtkGizmo (slider)
               reported min width/height -2»: el tema Breeze da a su pieza
               «slider» min 6 + borde 5+5 + margen -9-9 = -2 píxeles.
               Sin margen ni borde, el tamaño es siempre positivo.
               Sin «window.retro» delante: vale para todas las ventanas. */
            scrollbar slider {{
                margin: 0;
                border: none;
                border-radius: 0;
                box-shadow: none;
                background: {texto};
                min-width: 6px;
                min-height: 30px;
            }}
            scrollbar.horizontal slider {{
                min-width: 30px;
                min-height: 6px;
            }}

            /* Listas de los desplegables (perfiles, KeePass, fuentes...):
               por defecto el tema del sistema las pinta con fondo blanco. */
            window.retro popover scrolledwindow,
            window.retro popover viewport,
            window.retro popover listview,
            window.retro popover list,
            window.retro popover row {{
                background: {fondo};
                color: {texto};
            }}
            window.retro popover row label {{
                font-family: \"{fuente}\", monospace;
                font-size: 20px;
                color: {texto};
            }}
            window.retro popover row:hover,
            window.retro popover row:selected {{ background: {texto}; }}
            window.retro popover row:hover label,
            window.retro popover row:selected label {{ color: {fondo}; }}
            /* La marca ✓ de la opción elegida, del color del texto. */
            window.retro popover row image {{ color: inherit; }}
            window.retro .seccion {{ font-size: 24px; margin-top: 8px; }}
            ",
        fondo = tema.fondo,
        texto = tema.texto,
        cursor = tema.cursor,
        fuente = ajustes.fuente,
    )
}

/// Convierte "#33ff66" en un color de GTK.
///
/// Sin `pub`: es una función auxiliar privada de este módulo.
/// `-> gdk::RGBA` indica el tipo que devuelve. La última expresión
/// SIN punto y coma es el valor devuelto (no hace falta `return`).
fn color(hex: &str) -> gdk::RGBA {
    // `parse` devuelve un `Result`: o `Ok(color)` o `Err(error)`.
    // Si el color está mal escrito en un tema, paramos con un mensaje claro.
    gdk::RGBA::parse(hex).unwrap_or_else(|_| panic!("Color no válido en el tema: {hex}"))
}
