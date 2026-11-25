// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! El salvapantallas «Invasión»: una partida que se juega sola.
//!
//! - Una formación de alienígenas oscila a izquierda y derecha y va
//!   BAJANDO, cada vez más deprisa según se acerca al suelo (y según quedan
//!   menos).
//! - Un cañón automático, abajo, apunta al alienígena más bajo, calcula
//!   dónde estará cuando llegue la bala y dispara.
//! - Cada alienígena alcanzado estalla en sus propios cuadritos.
//! - Si el cañón acaba con todos: oleada nueva, algo más rápida. Si llegan
//!   al cañón: el cañón explota y la partida vuelve a empezar.
//!
//! Los alienígenas se INVENTAN al azar en cada bloqueo y en cada oleada:
//! una rejilla de 7×5 cuadritos cuya mitad izquierda se rellena al azar y
//! la derecha es su espejo (las figuras simétricas parecen «bichos»).
//!
//! Unidades: todo se mide en píxeles y segundos. La «unidad» `u` es el lado
//! de un cuadrito; se calcula según el tamaño de la pantalla.
//! La estela del fósforo la pone el `Lienzo` (salvapantallas.rs).

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{cairo, glib};
use vte4::gtk;

use crate::ajustes;
use crate::salvapantallas::{self, Lienzo};

// ---- Constantes de ajuste fino (para retocar cómo se juega) --------------

/// Pasos por segundo de la animación (unos 30, como la lluvia).
const PASO: Duration = Duration::from_millis(33);
/// La física avanza en trocitos de como mucho este tiempo (segundos). Si
/// un fotograma tarda más (pantalla enorme, equipo ocupado), se dan varios
/// trocitos seguidos: el juego va a su velocidad real aunque se vea a
/// saltos.
const PASO_FISICA: f64 = 1.0 / 60.0;
/// Pero si un fotograma tarda más que esto, no recuperamos más (evita que
/// tras un parón el juego «pegue un acelerón»).
const RETRASO_MAXIMO: f64 = 0.25;
/// Cuánto se apaga la estela en cada paso (más alto = estela más corta).
const DESVANECIMIENTO: f64 = 0.45;
/// Formación: columnas y filas de alienígenas.
const COLUMNAS: usize = 8;
const FILAS: usize = 4;
/// Tamaño de cada alienígena, en cuadritos.
const SPRITE_ANCHO: usize = 7;
const SPRITE_ALTO: usize = 5;
/// Distancia entre alienígenas (de esquina a esquina), en cuadritos.
const SEPARACION_X: f64 = 11.0;
const SEPARACION_Y: f64 = 9.0;
/// Segundos que tardaría la formación COMPLETA en llegar al cañón en la
/// primera oleada. Con menos alienígenas y en oleadas siguientes, menos.
const TIEMPO_BAJADA: f64 = 40.0;
/// Velocidad lateral de la formación, en anchos de pantalla por segundo.
const VELOCIDAD_LATERAL: f64 = 0.06;
/// Velocidad máxima del cañón, en anchos de pantalla por segundo.
const VELOCIDAD_CANON: f64 = 0.3;
/// Velocidad de las balas, en altos de pantalla por segundo.
const VELOCIDAD_BALA: f64 = 0.9;
/// Segundos mínimos entre dos disparos, y balas a la vez en pantalla.
const RECARGA: f64 = 0.35;
const MAX_BALAS: usize = 2;
/// Puntería: error al azar, en cuadritos (hace que a veces falle).
const ERROR_PUNTERIA: f64 = 2.0;
/// Pausas (segundos) tras ganar una oleada y tras perder la partida.
const PAUSA_OLEADA: f64 = 1.5;
const PAUSA_DERROTA: f64 = 3.0;
/// Hueco libre abajo para el rótulo «TERMINAL BLOQUEADO» (píxeles).
const HUECO_ROTULO: f64 = 66.0;

// ---- Tipos -----------------------------------------------------------------

/// La forma de una especie: la lista de cuadritos encendidos, como
/// (columna, fila) dentro de la rejilla de 7×5.
struct Especie {
    celdas: Vec<(usize, usize)>,
}

/// Un alienígena: su sitio DENTRO de la formación (en píxeles, desde la
/// esquina de la formación) y de qué especie es (posición en `especies`).
struct Alien {
    dx: f64,
    dy: f64,
    especie: usize,
}

/// Un cuadrito suelto de una explosión.
struct Fragmento {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    /// Segundos que le quedan, y los que tenía al nacer (para el brillo).
    vida: f64,
    vida_total: f64,
}

/// Todo lo de una partida. Separado de `Estado` para poder tomar prestados
/// a la vez el lienzo (para dibujar) y el juego (para leerlo).
struct Juego {
    ancho: f64,
    alto: f64,
    /// Lado de un cuadrito, en píxeles.
    u: f64,
    especies: Vec<Especie>,
    aliens: Vec<Alien>,
    /// Cuántos había al empezar la oleada (para saber cuántos quedan).
    total: usize,
    /// Esquina superior izquierda de la formación, y hacia dónde va (+1
    /// derecha, −1 izquierda).
    formacion_x: f64,
    formacion_y: f64,
    direccion: f64,
    /// Dónde acaba por abajo la formación al empezar (para medir el avance).
    fondo_inicial: f64,
    canon_x: f64,
    canon_vivo: bool,
    recarga: f64,
    /// Error de puntería del disparo que se está preparando.
    error: f64,
    /// Balas: (x, y) de su punta.
    balas: Vec<(f64, f64)>,
    fragmentos: Vec<Fragmento>,
    oleada: u32,
    /// Si es > 0, estamos en pausa (entre oleadas o tras perder).
    pausa: f64,
    derrota: bool,
}

struct Estado {
    lienzo: Lienzo,
    /// `None` hasta el primer dibujo (aún no sabemos el tamaño).
    juego: Option<Juego>,
    /// Lo pone a `true` la señal «map» (cada bloqueo): partida nueva.
    reiniciar: bool,
    ultimo_paso: Instant,
}

// ---- El widget -------------------------------------------------------------

/// Crea el área de dibujo de la invasión.
pub fn nuevo() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let estado = Rc::new(RefCell::new(Estado {
        lienzo: Lienzo::nuevo(),
        juego: None,
        reiniciar: true,
        ultimo_paso: Instant::now(),
    }));

    {
        let estado = estado.clone();
        area.set_draw_func(move |_area, cr, ancho, alto| {
            let mut estado = estado.borrow_mut();
            if let Err(error) = dibujar(&mut estado, cr, ancho, alto) {
                eprintln!("Error al dibujar la invasión: {error}");
            }
        });
    }
    // Cada vez que aparece (cada bloqueo): partida nueva, bichos nuevos.
    // El closure guarda el `Rc` del estado, no el área: no hay ciclo.
    area.connect_map(move |_| estado.borrow_mut().reiniciar = true);
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
    let tema = ajustes::actuales().tema();
    let fondo = salvapantallas::color(tema.fondo);
    let texto = salvapantallas::color(tema.texto);

    let recreado = estado.lienzo.preparar(ancho, alto, fondo)?;
    // `is_none()`: ¿todavía no hay partida?
    if recreado || estado.reiniciar || estado.juego.is_none() {
        estado.juego = Some(Juego::nuevo(ancho as f64, alto as f64));
        estado.reiniciar = false;
        estado.ultimo_paso = Instant::now();
    }

    let transcurrido = estado.ultimo_paso.elapsed();
    if transcurrido >= PASO {
        estado.ultimo_paso = Instant::now();
        // Segundos desde el paso anterior, repartidos en trocitos iguales de
        // como mucho PASO_FISICA. `ceil()` redondea hacia ARRIBA: 0,05 s
        // son 3 trocitos de 0,0167 s. `max(1)`: al menos uno.
        let tiempo = transcurrido.as_secs_f64().min(RETRASO_MAXIMO);
        let trocitos = ((tiempo / PASO_FISICA).ceil() as usize).max(1);
        let dt = tiempo / trocitos as f64;
        // `if let Some(juego) = &mut estado.juego`: préstamo MUTABLE de lo que
        // hay dentro del Option. Y a la vez, `estado.lienzo`: son campos
        // distintos de `estado`, y Rust deja prestarlos por separado.
        if let Some(juego) = &mut estado.juego {
            for _ in 0..trocitos {
                juego.actualizar(dt);
            }
            // El closure solo LEE `juego` (préstamo compartido, `&*juego`).
            let juego = &*juego;
            estado
                .lienzo
                .paso(DESVANECIMIENTO, |c| juego.pintar(c, texto))?;
        }
    }

    estado.lienzo.mostrar(cr)?;
    salvapantallas::pintar_encima(cr, ancho, alto)
}

// ---- La partida ------------------------------------------------------------

// `impl Juego { … }`: los MÉTODOS de `Juego`. Los que reciben `&self` solo
// leen; los de `&mut self` pueden cambiarlo; `nuevo` no recibe `self`: es
// una «función asociada» que se llama `Juego::nuevo(…)`.
impl Juego {
    /// Partida nueva para una pantalla de `ancho` × `alto`.
    fn nuevo(ancho: f64, alto: f64) -> Self {
        // El tamaño del cuadrito: que la formación ocupe un 60 % del ancho y
        // como mucho un 35 % del alto. `clamp`: ni diminuto ni enorme (el
        // tope alto es generoso para que en pantallas grandes no se quede
        // en un rincón).
        let ancho_formacion = (COLUMNAS as f64 - 1.0) * SEPARACION_X + SPRITE_ANCHO as f64;
        let alto_formacion = (FILAS as f64 - 1.0) * SEPARACION_Y + SPRITE_ALTO as f64;
        let u = (ancho * 0.6 / ancho_formacion)
            .min(alto * 0.35 / alto_formacion)
            .clamp(2.0, 24.0);
        // `Self { … }` = `Juego { … }`. Lo que se rellena en `nueva_oleada`
        // empieza con valores provisionales.
        let mut juego = Self {
            ancho,
            alto,
            u,
            especies: Vec::new(),
            aliens: Vec::new(),
            total: 0,
            formacion_x: 0.0,
            formacion_y: 0.0,
            direccion: 1.0,
            fondo_inicial: 0.0,
            canon_x: ancho / 2.0,
            canon_vivo: true,
            recarga: 0.0,
            error: 0.0,
            balas: Vec::new(),
            fragmentos: Vec::new(),
            oleada: 0,
            pausa: 0.0,
            derrota: false,
        };
        juego.nueva_oleada();
        juego
    }

    /// Formas nuevas y la formación arriba. La oleada sube en uno.
    fn nueva_oleada(&mut self) {
        self.oleada += 1;
        // 3 o 4 especies (`random_int_range(a, b)` da de a a b−1).
        let cuantas = glib::random_int_range(3, 5) as usize;
        self.especies = (0..cuantas).map(|_| especie_al_azar()).collect();

        let u = self.u;
        self.aliens.clear();
        for fila in 0..FILAS {
            // Cada fila, de una especie (si hay 3 especies y 4 filas, una se
            // repite: `fila % cuantas` vuelve a empezar por la 0).
            let especie = fila % cuantas;
            for columna in 0..COLUMNAS {
                self.aliens.push(Alien {
                    dx: columna as f64 * SEPARACION_X * u,
                    dy: fila as f64 * SEPARACION_Y * u,
                    especie,
                });
            }
        }
        self.total = self.aliens.len();

        let ancho_formacion = ((COLUMNAS as f64 - 1.0) * SEPARACION_X + SPRITE_ANCHO as f64) * u;
        let alto_formacion = ((FILAS as f64 - 1.0) * SEPARACION_Y + SPRITE_ALTO as f64) * u;
        self.formacion_x = (self.ancho - ancho_formacion) / 2.0;
        self.formacion_y = self.alto * 0.06;
        self.fondo_inicial = self.formacion_y + alto_formacion;
        // `if … { a } else { b }` es una EXPRESIÓN: da un valor.
        self.direccion = if glib::random_double() < 0.5 { 1.0 } else { -1.0 };
        self.balas.clear();
        self.recarga = 0.5; // un respiro antes del primer disparo
    }

    /// Línea del suelo (y), justo encima del rótulo.
    fn suelo(&self) -> f64 {
        self.alto - HUECO_ROTULO
    }

    /// Parte de arriba del cañón: si la formación llega aquí, ha ganado.
    fn tope_canon(&self) -> f64 {
        self.suelo() - 6.0 * self.u
    }

    /// Avanza la partida `dt` segundos.
    fn actualizar(&mut self, dt: f64) {
        self.mover_fragmentos(dt);

        // En pausa solo se mueven los fragmentos. Al acabar la pausa: oleada
        // nueva, o partida nueva si habíamos perdido.
        if self.pausa > 0.0 {
            self.pausa -= dt;
            if self.pausa <= 0.0 {
                if self.derrota {
                    // `*self = …`: sustituye la partida ENTERA por una nueva.
                    *self = Juego::nuevo(self.ancho, self.alto);
                } else {
                    self.nueva_oleada();
                }
            }
            return;
        }

        if self.aliens.is_empty() {
            self.pausa = PAUSA_OLEADA; // ¡oleada superada!
            return;
        }

        let velocidad_x = self.mover_formacion(dt);
        if self.pausa > 0.0 {
            return; // la formación acaba de llegar al cañón
        }
        self.mover_canon(dt, velocidad_x);
        self.mover_balas(dt);
    }

    /// Mueve la formación. Devuelve su velocidad lateral (con signo), que
    /// el cañón usa para apuntar.
    fn mover_formacion(&mut self, dt: f64) -> f64 {
        let u = self.u;
        // Los bordes de los que QUEDAN: así, sin las columnas de los
        // extremos, la formación llega más lejos (como debe ser).
        // `fold` recorre la lista acumulando un valor: aquí, el mínimo.
        let izquierda = self.aliens.iter().map(|a| a.dx).fold(f64::INFINITY, f64::min);
        let derecha = self
            .aliens
            .iter()
            .map(|a| a.dx + SPRITE_ANCHO as f64 * u)
            .fold(f64::NEG_INFINITY, f64::max);
        let abajo = self
            .aliens
            .iter()
            .map(|a| a.dy + SPRITE_ALTO as f64 * u)
            .fold(f64::NEG_INFINITY, f64::max);

        // Prisa: de 1 (todos vivos) a 2,5 (casi ninguno). Y cada oleada,
        // un 8 % más rápida que la anterior.
        let quedan = self.aliens.len() as f64 / self.total as f64;
        let prisa = 1.0 + 1.5 * (1.0 - quedan);
        let nivel = 1.0 + 0.08 * (self.oleada as f64 - 1.0);

        // De lado, rebotando en los márgenes.
        let margen = 2.0 * u;
        let velocidad_x = self.direccion * self.ancho * VELOCIDAD_LATERAL * prisa * nivel;
        self.formacion_x += velocidad_x * dt;
        if self.formacion_x + izquierda < margen {
            self.formacion_x = margen - izquierda;
            self.direccion = 1.0;
        } else if self.formacion_x + derecha > self.ancho - margen {
            self.formacion_x = self.ancho - margen - derecha;
            self.direccion = -1.0;
        }

        // Hacia abajo: `avance` va de 0 (arriba) a 1 (en el cañón), y la
        // velocidad crece con su cuadrado: lenta arriba, rápida al final.
        let tope = self.tope_canon();
        let distancia = (tope - self.fondo_inicial).max(1.0);
        let avance = ((self.formacion_y + abajo - self.fondo_inicial) / distancia).clamp(0.0, 1.0);
        let velocidad_y =
            distancia / TIEMPO_BAJADA * (0.5 + 2.5 * avance * avance) * prisa * nivel;
        self.formacion_y += velocidad_y * dt;

        // ¿Han llegado al cañón? Fin de la partida.
        if self.formacion_y + abajo >= tope {
            self.canon_vivo = false;
            self.derrota = true;
            self.pausa = PAUSA_DERROTA;
            let (x, y) = (self.canon_x - 6.5 * u, self.tope_canon());
            explotar(&mut self.fragmentos, &forma_canon(), x, y, u);
        }
        velocidad_x
    }

    /// El «piloto automático» del cañón: elegir blanco, moverse y disparar.
    fn mover_canon(&mut self, dt: f64, velocidad_x: f64) {
        let u = self.u;
        self.recarga -= dt;

        // Blanco: el más bajo (el más peligroso); si empatan, el más cercano.
        // `max_by` necesita saber comparar dos elementos. Los `f64` no se
        // pueden ordenar «del todo» (por los NaN), así que usamos
        // `partial_cmp`, y `then_with` para desempatar.
        let canon_x = self.canon_x;
        let centro = |a: &Alien| self.formacion_x + a.dx + SPRITE_ANCHO as f64 * u / 2.0;
        let Some(blanco) = self.aliens.iter().max_by(|a, b| {
            a.dy.partial_cmp(&b.dy).unwrap_or(Ordering::Equal).then_with(|| {
                // Más cercano = «mayor» en este orden, por eso va al revés.
                let da = (centro(a) - canon_x).abs();
                let db = (centro(b) - canon_x).abs();
                db.partial_cmp(&da).unwrap_or(Ordering::Equal)
            })
        }) else {
            return;
        };

        // ¿Dónde estará cuando llegue la bala? Tiempo de vuelo × su velocidad
        // lateral (sin contar rebotes: por eso a veces falla, y está bien).
        let y_blanco = self.formacion_y + blanco.dy + SPRITE_ALTO as f64 * u;
        let vuelo = (self.tope_canon() - y_blanco).max(0.0) / (self.alto * VELOCIDAD_BALA);
        let objetivo = centro(blanco) + velocidad_x * vuelo + self.error;

        // Moverse hacia allí, sin pasarse de su velocidad máxima.
        let paso_max = self.ancho * VELOCIDAD_CANON * dt;
        let diferencia = objetivo - self.canon_x;
        self.canon_x += diferencia.clamp(-paso_max, paso_max);
        let margen = 7.0 * u;
        self.canon_x = self.canon_x.clamp(margen, self.ancho - margen);

        // ¿Alineado, cargado y con balas libres? ¡Fuego!
        if diferencia.abs() < 1.5 * u && self.recarga <= 0.0 && self.balas.len() < MAX_BALAS {
            self.balas.push((self.canon_x, self.tope_canon() - u));
            self.recarga = RECARGA;
            // El error del PRÓXIMO disparo.
            self.error = glib::random_double_range(-ERROR_PUNTERIA, ERROR_PUNTERIA) * u;
        }
    }

    /// Sube las balas y mira si dan a alguien.
    fn mover_balas(&mut self, dt: f64) {
        let u = self.u;
        let subida = self.alto * VELOCIDAD_BALA * dt;
        let (fx, fy) = (self.formacion_x, self.formacion_y);
        // Prestamos POR SEPARADO los campos que hacen falta. Dentro del
        // `retain_mut` de `self.balas` no podríamos usar `self` entero (ya
        // está prestado), pero sí otros campos sueltos.
        let aliens = &mut self.aliens;
        let fragmentos = &mut self.fragmentos;
        let especies = &self.especies;

        // `retain_mut`: recorre la lista y se queda SOLO con las balas para
        // las que el closure devuelve `true`. Es la forma cómoda de borrar
        // elementos de un `Vec` mientras se recorre.
        self.balas.retain_mut(|(x, y)| {
            *y -= subida;
            if *y < 0.0 {
                return false; // salió por arriba
            }
            // ¿Toca a algún alienígena? (su rectángulo contiene la punta)
            let tocado = aliens.iter().position(|a| {
                let (ax, ay) = (fx + a.dx, fy + a.dy);
                *x >= ax
                    && *x <= ax + SPRITE_ANCHO as f64 * u
                    && *y >= ay
                    && *y <= ay + SPRITE_ALTO as f64 * u
            });
            match tocado {
                Some(i) => {
                    // `swap_remove`: quita el elemento i poniendo el último en
                    // su sitio. Más rápido que `remove` (no desplaza los
                    // demás); aquí el orden da igual.
                    let alien = aliens.swap_remove(i);
                    let forma = &especies[alien.especie].celdas;
                    explotar(fragmentos, forma, fx + alien.dx, fy + alien.dy, u);
                    false // la bala también desaparece
                }
                None => true,
            }
        });
    }

    fn mover_fragmentos(&mut self, dt: f64) {
        let gravedad = 40.0 * self.u;
        // `iter_mut()`: un `&mut` de cada fragmento, para cambiarlo.
        for f in self.fragmentos.iter_mut() {
            f.x += f.vx * dt;
            f.y += f.vy * dt;
            f.vy += gravedad * dt;
            f.vida -= dt;
        }
        // `retain`: se quedan solo los que aún viven.
        self.fragmentos.retain(|f| f.vida > 0.0);
    }

    /// Dibuja la partida en el lienzo.
    fn pintar(&self, c: &cairo::Context, color: (f64, f64, f64)) -> Result<(), cairo::Error> {
        let u = self.u;
        // Las balas, más brillantes (mezcla con blanco).
        let brillo = (
            (color.0 + 1.0) / 2.0,
            (color.1 + 1.0) / 2.0,
            (color.2 + 1.0) / 2.0,
        );

        // Alienígenas: cada cuadrito un poco más pequeño que la celda (deja
        // una rendija que da aspecto de «píxel gordo»).
        c.set_source_rgb(color.0, color.1, color.2);
        for alien in &self.aliens {
            let (x, y) = (self.formacion_x + alien.dx, self.formacion_y + alien.dy);
            for &(col, fila) in &self.especies[alien.especie].celdas {
                c.rectangle(x + col as f64 * u, y + fila as f64 * u, u * 0.9, u * 0.9);
            }
        }
        c.fill()?;

        // El cañón.
        if self.canon_vivo {
            let (x, y) = (self.canon_x - 6.5 * u, self.tope_canon());
            for (col, fila) in forma_canon() {
                c.rectangle(x + col as f64 * u, y + fila as f64 * u, u * 0.9, u * 0.9);
            }
            c.fill()?;
        }

        // El suelo: una línea tenue.
        c.set_source_rgba(color.0, color.1, color.2, 0.5);
        c.rectangle(0.0, self.suelo() + u * 0.5, self.ancho, (u / 2.0).max(1.0));
        c.fill()?;

        // Balas.
        c.set_source_rgb(brillo.0, brillo.1, brillo.2);
        for &(x, y) in &self.balas {
            c.rectangle(x - u / 2.0, y, u, 3.0 * u);
        }
        c.fill()?;

        // Fragmentos: se apagan según se les acaba la vida.
        for f in &self.fragmentos {
            let alfa = (f.vida / f.vida_total).clamp(0.0, 1.0);
            c.set_source_rgba(brillo.0, brillo.1, brillo.2, alfa);
            c.rectangle(f.x, f.y, u * 0.9, u * 0.9);
            c.fill()?;
        }
        Ok(())
    }
}

// ---- Funciones sueltas -----------------------------------------------------

/// Inventa una especie: mitad izquierda al azar, derecha en espejo.
fn especie_al_azar() -> Especie {
    let mitad = SPRITE_ANCHO / 2; // 3: la columna central
    // `loop` repite hasta un `return`: probamos hasta que salga un bicho
    // «decente» (casi siempre a la primera o a la segunda).
    loop {
        let mut celdas = Vec::new();
        for fila in 0..SPRITE_ALTO {
            // `0..=mitad`: de 0 a 3, AMBOS incluidos (el `=`).
            for col in 0..=mitad {
                if glib::random_double() < 0.5 {
                    celdas.push((col, fila));
                    if col != mitad {
                        celdas.push((SPRITE_ANCHO - 1 - col, fila)); // su espejo
                    }
                }
            }
        }
        // Decente = bastantes cuadritos y ninguna fila vacía (si no, salen
        // trozos sueltos que no parecen un bicho).
        // `all(…)`: ¿TODAS las filas cumplen? `any(…)`: ¿ALGUNA celda está en ella?
        let filas_llenas = (0..SPRITE_ALTO).all(|f| celdas.iter().any(|&(_, y)| y == f));
        if celdas.len() >= 13 && filas_llenas {
            return Especie { celdas };
        }
    }
}

/// La forma del cañón (13×6 cuadritos): cañón, cuerpo y base.
fn forma_canon() -> Vec<(usize, usize)> {
    let mut celdas = Vec::new();
    // (fila, desde la columna, hasta la columna) de cada tramo.
    let tramos = [(0, 6, 6), (1, 6, 6), (2, 4, 8), (3, 1, 11), (4, 0, 12), (5, 0, 12)];
    for (fila, desde, hasta) in tramos {
        for col in desde..=hasta {
            celdas.push((col, fila));
        }
    }
    celdas
}

/// Convierte una forma en fragmentos que salen despedidos desde su centro.
/// `forma: &[(usize, usize)]` acepta tanto un `&Vec` como un array.
fn explotar(fragmentos: &mut Vec<Fragmento>, forma: &[(usize, usize)], x: f64, y: f64, u: f64) {
    // El centro de la forma, para empujar cada trozo hacia fuera.
    let n = forma.len().max(1) as f64;
    let cx = forma.iter().map(|&(c, _)| c as f64).sum::<f64>() / n;
    let cy = forma.iter().map(|&(_, f)| f as f64).sum::<f64>() / n;
    for &(col, fila) in forma {
        let (dx, dy) = (col as f64 - cx, fila as f64 - cy);
        let fuerza = glib::random_double_range(6.0, 16.0) * u;
        let vida = glib::random_double_range(0.5, 1.1);
        fragmentos.push(Fragmento {
            x: x + col as f64 * u,
            y: y + fila as f64 * u,
            // Hacia fuera, más un poco al azar y un empujón hacia arriba.
            vx: dx * fuerza / 2.0 + glib::random_double_range(-4.0, 4.0) * u,
            vy: dy * fuerza / 2.0 - glib::random_double_range(4.0, 12.0) * u,
            vida,
            vida_total: vida,
        });
    }
}

// ---- Pruebas: `cargo test invasion -- --nocapture` muestra el resumen ------

#[cfg(test)]
mod tests {
    use super::*;

    /// Juega 20 minutos simulados (sin pantalla) y devuelve (oleadas
    /// ganadas, oleadas en las que se perdió cada partida). Sirve para
    /// afinar las constantes: el cañón debe ganar casi siempre, no siempre.
    fn simular(ancho: f64, alto: f64) -> (u32, Vec<u32>) {
        let mut juego = Juego::nuevo(ancho, alto);
        let mut ganadas = 0;
        let mut caidas = Vec::new();
        let (mut derrota_antes, mut oleada_antes) = (false, juego.oleada);
        // 20 min × 60 s / 0,033 s ≈ 36 000 pasos.
        for _ in 0..36_000 {
            juego.actualizar(0.033);
            if juego.derrota && !derrota_antes {
                caidas.push(juego.oleada);
            }
            if juego.oleada > oleada_antes {
                ganadas += 1;
            }
            derrota_antes = juego.derrota;
            oleada_antes = juego.oleada;
        }
        (ganadas, caidas)
    }

    #[test]
    fn simulacion_equilibrada() {
        // Pantalla grande y portátil.
        for (ancho, alto) in [(1920.0, 1080.0), (1366.0, 768.0)] {
            let (ganadas, caidas) = simular(ancho, alto);
            eprintln!("Invasión {ancho}×{alto}, 20 min: {ganadas} oleadas ganadas; partidas perdidas en las oleadas {caidas:?}");
            assert!(ganadas > 0, "el cañón no gana nunca");
            assert!(ganadas as usize > caidas.len(), "el cañón pierde demasiado");
        }
    }
}

#[cfg(test)]
mod medida {
    use super::*;
    /// Cuánto tarda un paso de dibujo a varios tamaños. No se ejecuta con
    /// `cargo test` normal (`#[ignore]`); para medir:
    /// `cargo test medida -- --ignored --nocapture` (y con `--release`).
    #[test]
    #[ignore]
    fn tiempo_por_paso() {
        for (ancho, alto) in [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)] {
            let mut lienzo = Lienzo::nuevo();
            lienzo.preparar(ancho, alto, (0.0, 0.0, 0.0)).unwrap();
            let mut juego = Juego::nuevo(ancho as f64, alto as f64);
            let pantalla = cairo::ImageSurface::create(cairo::Format::Rgb24, ancho, alto).unwrap();
            let cr = cairo::Context::new(&pantalla).unwrap();
            let inicio = Instant::now();
            for _ in 0..60 {
                juego.actualizar(0.033);
                let j = &juego;
                lienzo.paso(DESVANECIMIENTO, |c| j.pintar(c, (0.2, 1.0, 0.4))).unwrap();
                lienzo.mostrar(&cr).unwrap();
            }
            eprintln!("{ancho}×{alto}: {:.1} ms por paso", inicio.elapsed().as_secs_f64() * 1000.0 / 60.0);
        }
    }
}
