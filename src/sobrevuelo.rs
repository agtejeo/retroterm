// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! El salvapantallas «Sobrevuelo»: un vuelo sin fin sobre un terreno de
//! polígonos rellenos, como los simuladores de los años 80.
//!
//! Las piezas:
//!
//! 1. **El terreno** es una FUNCIÓN: `altura(x, z)` da la altura de
//!    cualquier punto, sumando varias capas de «ruido» suavizado (grandes
//!    ondulaciones + detalles más pequeños). Como se calcula, no se guarda:
//!    el terreno no se acaba nunca. Lo que queda por debajo de un nivel es
//!    agua: lagos planos en el fondo de los valles. La semilla del ruido es
//!    al azar en cada bloqueo: cada vez, otro paisaje.
//! 2. **La cámara** avanza sin parar, gira a derecha e izquierda con curvas
//!    suaves (alabeándose, como un avión) y sigue el relieve a cierta
//!    altura, mirando un poco hacia abajo.
//! 3. **El dibujo**, en cada fotograma: una rejilla de casillas alrededor de
//!    la cámara, cada una en 2 triángulos; se proyectan en perspectiva y se
//!    pintan DE ATRÁS HACIA DELANTE («algoritmo del pintor»: lo cercano
//!    tapa lo lejano sin calcular qué se ve). Cada triángulo, relleno con un
//!    tono del fósforo según le dé la luz, con sus aristas algo más claras y
//!    fundido con el fondo a lo lejos («niebla»).
//!
//! Coordenadas del mundo: `x` hacia el este, `z` hacia el norte, `y` hacia
//! arriba. La unidad es el lado de una casilla.

use std::cell::RefCell;
use std::f64::consts::PI;
// Los traits de los operadores: implementarlos para `Vec3` permite
// escribir `a + b`, `a - b` y `a * 2.0` con nuestros propios vectores.
use std::ops::{Add, Mul, Sub};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{cairo, glib};
use vte4::gtk;

use crate::ajustes;
use crate::salvapantallas;

// ---- Constantes de ajuste fino ---------------------------------------------

/// Cada cuánto se calcula y dibuja una escena nueva (unas 30 por segundo,
/// como los otros salvapantallas). Entre medias se reutiliza la última.
const PASO: Duration = Duration::from_millis(33);
/// Casillas hasta donde se ve, en cada dirección. La rejilla es de
/// (2·ALCANCE)² casillas, pero solo se dibujan las de delante y dentro de
/// este radio (más allá, «niebla»).
const ALCANCE: i32 = 26;
/// Altura máxima de las montañas (en casillas).
const AMPLITUD: f64 = 10.0;
/// Tamaño de las ondulaciones grandes (casillas por «colina»).
const ESCALA: f64 = 16.0;
/// Capas de ruido: cada una con el doble de detalle y la mitad de altura.
const CAPAS: u32 = 4;
/// Altura del agua: por debajo, lago plano.
const NIVEL_AGUA: f64 = 0.4;
/// Velocidad de vuelo (casillas por segundo).
const VELOCIDAD: f64 = 5.0;
/// Altura sobre el terreno que intenta mantener la cámara.
const ALTURA_VUELO: f64 = 2.8;
/// Cuánto mira hacia abajo la cámara (radianes; 0,12 ≈ 7°).
const CABECEO: f64 = 0.12;
/// Giro máximo (radianes por segundo) y alabeo por cada unidad de giro.
const GIRO_MAX: f64 = 0.4;
const ALABEO_POR_GIRO: f64 = 1.1;
/// Lo que esté más cerca que esto (en profundidad) no se dibuja.
const CERCA: f64 = 0.3;
/// La luz: de dónde viene (arriba, a la izquierda y algo por delante).
const LUZ: Vec3 = Vec3 { x: -0.5, y: 0.8, z: 0.3 };

// ---- Vectores en 3D --------------------------------------------------------

/// Un punto (o una dirección) en 3D.
///
/// `#[derive(Clone, Copy)]`: es pequeño (3 números), así que lo hacemos
/// `Copy`: al pasarlo o asignarlo se COPIA, como un número, en vez de
/// «moverse». Así podemos usar `a` después de escribir `a + b`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    /// `const fn`: se puede usar para crear constantes (como `LUZ`).
    const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    /// Producto escalar: cuánto «apuntan hacia el mismo sitio» dos vectores.
    fn punto(self, otro: Vec3) -> f64 {
        self.x * otro.x + self.y * otro.y + self.z * otro.z
    }

    /// Producto vectorial: un vector perpendicular a los dos (la «normal»
    /// de un triángulo sale de aquí).
    fn cruz(self, otro: Vec3) -> Vec3 {
        Vec3::new(
            self.y * otro.z - self.z * otro.y,
            self.z * otro.x - self.x * otro.z,
            self.x * otro.y - self.y * otro.x,
        )
    }

    /// El mismo vector con longitud 1 (o él mismo si mide 0).
    fn unitario(self) -> Vec3 {
        let largo = self.punto(self).sqrt();
        if largo > 0.0 {
            self * (1.0 / largo)
        } else {
            self
        }
    }
}

// Sobrecarga de operadores: `impl Add for Vec3` le enseña a Rust qué
// significa `a + b` para dos `Vec3`. `type Output` es el tipo del
// resultado. Igual con `-` (Sub) y con `* número` (Mul<f64>).
impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, otro: Vec3) -> Vec3 {
        Vec3::new(self.x + otro.x, self.y + otro.y, self.z + otro.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, otro: Vec3) -> Vec3 {
        Vec3::new(self.x - otro.x, self.y - otro.y, self.z - otro.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f64) -> Vec3 {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
}

// ---- El terreno ------------------------------------------------------------

/// Un número «al azar» entre 0 y 1, pero siempre el MISMO para la misma
/// esquina (ix, iz) y semilla: es una función «hash» (picadillo de bits).
///
/// `wrapping_mul`: multiplicar dejando que se desborde (se queda con los
/// 32 bits bajos). En Rust, desbordar un entero con `*` normal es un error
/// en modo depuración; aquí lo queremos a propósito. `^` es el O exclusivo
/// bit a bit y `>>` desplaza los bits a la derecha: mezclan bien los bits.
fn azar(ix: i32, iz: i32, semilla: u32) -> f64 {
    let mut h = (ix as u32).wrapping_mul(0x27d4_eb2d)
        ^ (iz as u32).wrapping_mul(0x1656_67b1)
        ^ semilla.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h as f64 / u32::MAX as f64
}

/// «Ruido de valores»: números al azar en las esquinas de una rejilla y,
/// entre ellas, una mezcla suave. Da un relieve ondulado, sin escalones.
fn ruido(x: f64, z: f64, semilla: u32) -> f64 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    // Dónde estamos dentro de la casilla (de 0 a 1)…
    let (fx, fz) = (x - x.floor(), z - z.floor());
    // …suavizado con la curva «smoothstep» (3t² − 2t³): así la pendiente
    // no cambia de golpe al pasar de una casilla a otra.
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let mezcla = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let arriba = mezcla(azar(ix, iz, semilla), azar(ix + 1, iz, semilla), sx);
    let abajo = mezcla(azar(ix, iz + 1, semilla), azar(ix + 1, iz + 1, semilla), sx);
    mezcla(arriba, abajo, sz)
}

/// La altura del terreno en (x, z). Nunca baja de NIVEL_AGUA (el agua).
fn altura(x: f64, z: f64, semilla: u32) -> f64 {
    // Suma de capas: cada una el doble de fina y la mitad de alta.
    let (mut suma, mut peso, mut total, mut frecuencia) = (0.0, 1.0, 0.0, 1.0 / ESCALA);
    for capa in 0..CAPAS {
        // Cada capa con otra semilla, para que no se parezcan entre sí.
        suma += ruido(x * frecuencia, z * frecuencia, semilla.wrapping_add(capa)) * peso;
        total += peso;
        peso *= 0.5;
        frecuencia *= 2.0;
    }
    // `suma / total` queda casi siempre entre 0,3 y 0,7: lo estiramos a 0-1
    // y lo elevamos al cuadrado (valles anchos, cumbres más puntiagudas).
    // Lo que queda por debajo de 0,29 (y algo más, por el cuadrado) es agua.
    let n = ((suma / total - 0.29) / 0.42).clamp(0.0, 1.0);
    (AMPLITUD * n * n).max(NIVEL_AGUA)
}

// ---- La cámara y el vuelo --------------------------------------------------

struct Estado {
    semilla: u32,
    /// Posición de la cámara.
    posicion: Vec3,
    /// Hacia dónde mira (radianes; 0 = hacia +z, positivo = a la derecha).
    rumbo: f64,
    /// Velocidad de giro actual y la que busca (radianes por segundo).
    giro: f64,
    giro_objetivo: f64,
    /// Segundos que quedan para elegir otra curva.
    cambio_en: f64,
    ultimo: Instant,
    /// Lo pone a `true` la señal «map» (cada bloqueo): vuelo nuevo.
    reiniciar: bool,
    /// Los triángulos de cada fotograma. Se reutiliza (con `clear()`) para
    /// no pedir memoria nueva 30 veces por segundo.
    triangulos: Vec<Triangulo>,
    /// La última escena dibujada. `None` hasta la primera.
    imagen: Option<cairo::ImageSurface>,
}

/// Un triángulo ya proyectado en la pantalla, listo para pintar.
struct Triangulo {
    puntos: [(f64, f64); 3],
    /// Profundidad (para ordenar: primero los más lejanos).
    profundidad: f64,
    relleno: (f64, f64, f64),
    borde: (f64, f64, f64),
}

impl Estado {
    fn nuevo() -> Self {
        Estado {
            semilla: 0,
            posicion: Vec3::new(0.0, 0.0, 0.0),
            rumbo: 0.0,
            giro: 0.0,
            giro_objetivo: 0.0,
            cambio_en: 0.0,
            ultimo: Instant::now(),
            reiniciar: true,
            triangulos: Vec::new(),
            imagen: None,
        }
    }

    /// Paisaje nuevo: semilla al azar y la cámara en el origen.
    fn empezar(&mut self) {
        // `random_int()` da un u32 al azar.
        self.semilla = glib::random_int();
        let suelo = altura(0.0, 0.0, self.semilla);
        self.posicion = Vec3::new(0.0, suelo + ALTURA_VUELO, 0.0);
        self.rumbo = glib::random_double_range(0.0, 2.0 * PI);
        self.giro = 0.0;
        self.giro_objetivo = 0.0;
        self.cambio_en = 2.0;
        self.ultimo = Instant::now();
        self.reiniciar = false;
    }

    /// Hacia dónde está «delante» (en el plano: sin subir ni bajar).
    fn delante(&self) -> Vec3 {
        Vec3::new(self.rumbo.sin(), 0.0, self.rumbo.cos())
    }

    /// Avanza el vuelo `dt` segundos.
    fn avanzar(&mut self, dt: f64) {
        // Curvas: de vez en cuando se elige otro giro (a veces, recto).
        self.cambio_en -= dt;
        if self.cambio_en <= 0.0 {
            self.cambio_en = glib::random_double_range(3.0, 8.0);
            self.giro_objetivo = if glib::random_double() < 0.25 {
                0.0
            } else {
                glib::random_double_range(-GIRO_MAX, GIRO_MAX)
            };
        }
        // Acercarse poco a poco al giro buscado: entrar y salir de las
        // curvas con suavidad. `min(1.0)`: nunca pasarse.
        self.giro += (self.giro_objetivo - self.giro) * (dt * 0.8).min(1.0);
        self.rumbo += self.giro * dt;
        self.posicion = self.posicion + self.delante() * (VELOCIDAD * dt);

        // Altura: mirar el terreno de los próximos metros (para subir ANTES
        // de llegar a la montaña) y tender a ALTURA_VUELO por encima.
        // `(0..6).map(…).fold(…)`: la altura máxima de 6 puntos por delante.
        let delante = self.delante();
        let mas_alto = (0..6)
            .map(|i| {
                let p = self.posicion + delante * (i as f64 * 2.0);
                altura(p.x, p.z, self.semilla)
            })
            .fold(f64::NEG_INFINITY, f64::max);
        let objetivo = mas_alto + ALTURA_VUELO;
        self.posicion.y += (objetivo - self.posicion.y) * (dt * 1.2).min(1.0);
        // Y nunca, nunca, atravesar el suelo que hay justo debajo.
        let debajo = altura(self.posicion.x, self.posicion.z, self.semilla);
        self.posicion.y = self.posicion.y.max(debajo + 1.0);
    }
}

/// Lo que hace falta para pasar un punto del mundo a la pantalla. Se
/// calcula una vez por fotograma (senos y cosenos incluidos).
struct Camara {
    posicion: Vec3,
    seno_rumbo: f64,
    coseno_rumbo: f64,
    seno_cabeceo: f64,
    coseno_cabeceo: f64,
    seno_alabeo: f64,
    coseno_alabeo: f64,
    /// Centro de la proyección en la pantalla y «zoom» (distancia focal).
    centro: (f64, f64),
    focal: f64,
}

impl Camara {
    fn de(estado: &Estado, ancho: f64, alto: f64) -> Self {
        // Alabeo: al girar a la derecha, el avión se inclina a la derecha.
        let alabeo = (estado.giro * ALABEO_POR_GIRO).clamp(-0.5, 0.5);
        Camara {
            posicion: estado.posicion,
            seno_rumbo: estado.rumbo.sin(),
            coseno_rumbo: estado.rumbo.cos(),
            seno_cabeceo: CABECEO.sin(),
            coseno_cabeceo: CABECEO.cos(),
            seno_alabeo: alabeo.sin(),
            coseno_alabeo: alabeo.cos(),
            centro: (ancho / 2.0, alto * 0.42),
            focal: ancho * 0.6,
        }
    }

    /// Del mundo al «espacio de la cámara»: x a la derecha, y arriba, z la
    /// profundidad (hacia delante). Son tres giros seguidos: rumbo,
    /// cabeceo (mirar abajo) y alabeo (inclinarse).
    fn a_camara(&self, p: Vec3) -> Vec3 {
        let d = p - self.posicion;
        // Rumbo (giro alrededor del eje vertical).
        let x = d.x * self.coseno_rumbo - d.z * self.seno_rumbo;
        let z = d.x * self.seno_rumbo + d.z * self.coseno_rumbo;
        // Cabeceo: mirar hacia abajo sube en la imagen lo que hay delante.
        let y = d.y * self.coseno_cabeceo + z * self.seno_cabeceo;
        let z = -d.y * self.seno_cabeceo + z * self.coseno_cabeceo;
        // Alabeo: el mundo gira en sentido contrario al avión.
        let (x, y) = (
            x * self.coseno_alabeo - y * self.seno_alabeo,
            x * self.seno_alabeo + y * self.coseno_alabeo,
        );
        Vec3::new(x, y, z)
    }

    /// Perspectiva: lo lejano (z grande) se acerca al centro. En la
    /// pantalla, la y crece hacia ABAJO; de ahí el signo menos.
    fn proyectar(&self, c: Vec3) -> (f64, f64) {
        (
            self.centro.0 + self.focal * c.x / c.z,
            self.centro.1 - self.focal * c.y / c.z,
        )
    }
}

// ---- El widget -------------------------------------------------------------

/// Crea el área de dibujo del sobrevuelo.
pub fn nuevo() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let estado = Rc::new(RefCell::new(Estado::nuevo()));
    {
        let estado = estado.clone();
        area.set_draw_func(move |_area, cr, ancho, alto| {
            let mut estado = estado.borrow_mut();
            if let Err(error) = dibujar(&mut estado, cr, ancho, alto) {
                eprintln!("Error al dibujar el sobrevuelo: {error}");
            }
        });
    }
    // Cada bloqueo, un paisaje nuevo.
    area.connect_map(move |_| estado.borrow_mut().reiniciar = true);
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
    if estado.reiniciar {
        estado.empezar();
        estado.imagen = None;
    }
    // ¿Hay que dibujar una escena nueva? Si no hay ninguna, si cambió el
    // tamaño de la ventana o si ya toca (cada PASO). `map_or(true, …)`: con
    // `None` da `true`; con `Some(imagen)`, lo que diga el closure.
    let otro_tamano = estado
        .imagen
        .as_ref()
        .map_or(true, |imagen| imagen.width() != ancho || imagen.height() != alto);
    if otro_tamano || estado.ultimo.elapsed() >= PASO {
        // Avanzar el vuelo el tiempo REAL transcurrido. `min(0.1)`: tras un
        // parón, sin saltos.
        let dt = estado.ultimo.elapsed().as_secs_f64().min(0.1);
        estado.ultimo = Instant::now();
        estado.avanzar(dt);

        let tema = ajustes::actuales().tema();
        let fondo = salvapantallas::color(tema.fondo);
        let texto = salvapantallas::color(tema.texto);
        // `take()` SACA la imagen del Option (que se queda en `None`): así
        // podemos pasar `estado` entero como `&mut` a `pintar_escena` sin
        // que Rust proteste porque la imagen está dentro. Luego se devuelve.
        let imagen = match estado.imagen.take() {
            Some(imagen) if !otro_tamano => imagen,
            _ => cairo::ImageSurface::create(cairo::Format::Rgb24, ancho, alto)?,
        };
        {
            // El Context se suelta al cerrar el bloque (ver salvapantallas.rs).
            let c = cairo::Context::new(&imagen)?;
            pintar_escena(estado, &c, ancho as f64, alto as f64, fondo, texto)?;
        }
        estado.imagen = Some(imagen);
    }

    if let Some(imagen) = &estado.imagen {
        cr.set_source_surface(imagen, 0.0, 0.0)?;
        cr.paint()?;
        // Que `cr` suelte su referencia a la imagen (ver Lienzo::mostrar).
        cr.set_source_rgb(0.0, 0.0, 0.0);
    }
    salvapantallas::pintar_encima(cr, ancho, alto)
}

/// Dibuja el cielo, el horizonte y el terreno.
fn pintar_escena(
    estado: &mut Estado,
    cr: &cairo::Context,
    ancho: f64,
    alto: f64,
    fondo: (f64, f64, f64),
    texto: (f64, f64, f64),
) -> Result<(), cairo::Error> {
    let camara = Camara::de(estado, ancho, alto);

    // El cielo: liso, del color de fondo.
    cr.set_source_rgb(fondo.0, fondo.1, fondo.2);
    cr.paint()?;

    // El horizonte: dos puntos muy lejanos a la altura de la cámara, uno a
    // cada lado. Con el alabeo, la línea sale inclinada.
    let delante = estado.delante();
    let derecha = Vec3::new(delante.z, 0.0, -delante.x);
    let lejos = camara.posicion + delante * 10_000.0;
    let a = camara.proyectar(camara.a_camara(lejos - derecha * 30_000.0));
    let b = camara.proyectar(camara.a_camara(lejos + derecha * 30_000.0));
    let horizonte = mezclar(fondo, texto, 0.35);
    cr.set_source_rgb(horizonte.0, horizonte.1, horizonte.2);
    cr.set_line_width(1.0);
    cr.move_to(a.0, a.1);
    cr.line_to(b.0, b.1);
    cr.stroke()?;

    // ---- La rejilla de alturas alrededor de la cámara. Pegada a los
    // números enteros del mundo: así los vértices no «nadan» al avanzar.
    let semilla = estado.semilla;
    let base_x = camara.posicion.x.floor() as i32 - ALCANCE;
    let base_z = camara.posicion.z.floor() as i32 - ALCANCE;
    let lado = (2 * ALCANCE + 1) as usize; // vértices por lado
    // Un `Vec` plano usado como tabla de 2 dimensiones: la casilla
    // (i, j) está en la posición `j * lado + i`.
    let mut alturas = Vec::with_capacity(lado * lado);
    for j in 0..lado {
        for i in 0..lado {
            let (x, z) = ((base_x + i as i32) as f64, (base_z + j as i32) as f64);
            alturas.push(altura(x, z, semilla));
        }
    }
    // Un closure que da el punto 3D del vértice (i, j).
    let vertice = |i: usize, j: usize| {
        Vec3::new(
            (base_x + i as i32) as f64,
            alturas[j * lado + i],
            (base_z + j as i32) as f64,
        )
    };

    let luz = LUZ.unitario();
    let radio = ALCANCE as f64;
    let triangulos = &mut estado.triangulos;
    triangulos.clear();
    for j in 0..lado - 1 {
        for i in 0..lado - 1 {
            // Las 4 esquinas de la casilla y sus 2 triángulos.
            let (p00, p10) = (vertice(i, j), vertice(i + 1, j));
            let (p01, p11) = (vertice(i, j + 1), vertice(i + 1, j + 1));
            for tri in [[p00, p10, p01], [p10, p11, p01]] {
                if let Some(t) = preparar(&camara, tri, luz, radio, fondo, texto) {
                    triangulos.push(t);
                }
            }
        }
    }

    // Algoritmo del pintor: primero los más lejanos. `sort_unstable_by`
    // ordena con nuestra comparación (al revés: `b` con `a`, para que el
    // mayor vaya primero). `total_cmp` compara f64 sin problemas con NaN.
    triangulos.sort_unstable_by(|a, b| b.profundidad.total_cmp(&a.profundidad));

    cr.set_line_width((ancho / 1600.0).max(1.0));
    cr.set_line_join(cairo::LineJoin::Round);
    // Sin suavizado de bordes: con él, Cairo tardaba 3-4 veces más (unos
    // 20 ms por fotograma en 1080p; sin él, 6). Y los bordes «en
    // escalera» son justo lo que se veía en los 80.
    cr.set_antialias(cairo::Antialias::None);
    for t in triangulos.iter() {
        let [a, b, c] = t.puntos;
        cr.move_to(a.0, a.1);
        cr.line_to(b.0, b.1);
        cr.line_to(c.0, c.1);
        cr.close_path();
        cr.set_source_rgb(t.relleno.0, t.relleno.1, t.relleno.2);
        // `fill_preserve`: rellena SIN borrar el trazado, para que el
        // `stroke` de después dibuje las aristas del mismo triángulo.
        cr.fill_preserve()?;
        cr.set_source_rgb(t.borde.0, t.borde.1, t.borde.2);
        cr.stroke()?;
    }
    Ok(())
}

/// Prepara un triángulo del mundo para pintarlo: lo proyecta y calcula sus
/// colores. `None` si no se ve (detrás de la cámara o fuera del alcance).
fn preparar(
    camara: &Camara,
    tri: [Vec3; 3],
    luz: Vec3,
    radio: f64,
    fondo: (f64, f64, f64),
    texto: (f64, f64, f64),
) -> Option<Triangulo> {
    let [a, b, c] = tri;
    // Lejos (en el plano) → no se dibuja: es la «niebla» total.
    let centro = (a + b + c) * (1.0 / 3.0);
    let (dx, dz) = (centro.x - camara.posicion.x, centro.z - camara.posicion.z);
    let distancia = (dx * dx + dz * dz).sqrt();
    if distancia > radio {
        return None;
    }
    // A la cámara. Si algún vértice queda detrás (o casi), fuera.
    // `map` sobre un array da otro array del mismo tamaño.
    let enc = tri.map(|p| camara.a_camara(p));
    if enc.iter().any(|p| p.z <= CERCA) {
        return None;
    }

    // La normal (perpendicular) del triángulo, hacia arriba. Cuanto más de
    // frente le dé la luz (producto escalar cerca de 1), más claro.
    let mut normal = (b - a).cruz(c - a).unitario();
    if normal.y < 0.0 {
        normal = normal * -1.0;
    }
    let agua = a.y <= NIVEL_AGUA && b.y <= NIVEL_AGUA && c.y <= NIVEL_AGUA;
    let tono = if agua {
        0.18 // el agua: oscura y lisa
    } else {
        // Ni negro del todo en sombra ni fósforo puro al sol (deslumbra).
        0.12 + 0.68 * normal.punto(luz).max(0.0)
    };
    // Niebla: a lo lejos, cada vez más del color de fondo.
    let cercania = 1.0 - (distancia / radio).powi(2);
    let relleno = mezclar(fondo, mezclar(fondo, texto, tono), cercania);
    // Las aristas, algo más claras que el relleno (en el agua, discretas).
    let realce = if agua { 0.12 } else { 0.35 };
    let borde = mezclar(relleno, mezclar(fondo, texto, cercania), realce);

    Some(Triangulo {
        puntos: enc.map(|p| camara.proyectar(p)),
        // La profundidad del más lejano de sus vértices (un truco clásico
        // que suele ordenar mejor que la media).
        profundidad: enc.iter().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max),
        relleno,
        borde,
    })
}

/// Mezcla dos colores: `t` = 0 da `a`, 1 da `b`.
fn mezclar(a: (f64, f64, f64), b: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    (
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

// ---- Pruebas ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operadores_de_vec3() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(b - a, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(a.punto(b), 32.0);
        // x × z = −y (regla de la mano derecha).
        let x = Vec3::new(1.0, 0.0, 0.0);
        let z = Vec3::new(0.0, 0.0, 1.0);
        assert_eq!(x.cruz(z), Vec3::new(0.0, -1.0, 0.0));
    }

    #[test]
    fn terreno_determinista_y_en_rango() {
        // Misma semilla y punto → misma altura; y siempre entre el agua y
        // la cumbre máxima.
        for k in 0..1000 {
            let (x, z) = (k as f64 * 0.37, k as f64 * -0.73);
            let h = altura(x, z, 1234);
            assert_eq!(h, altura(x, z, 1234));
            assert!((NIVEL_AGUA..=AMPLITUD).contains(&h), "altura {h} fuera de rango");
        }
    }

    #[test]
    fn hay_valles_con_agua_y_montanas() {
        // En una zona grande tiene que haber de todo un poco.
        let alturas: Vec<f64> = (0..10_000)
            .map(|k| altura((k % 100) as f64, (k / 100) as f64, 42))
            .collect();
        let agua = alturas.iter().filter(|&&h| h <= NIVEL_AGUA).count();
        let altas = alturas.iter().filter(|&&h| h > AMPLITUD * 0.4).count();
        eprintln!("agua: {agua} / 10000, montaña: {altas} / 10000");
        assert!(agua > 100, "no hay lagos");
        assert!(altas > 100, "no hay montañas");
    }

    #[test]
    fn la_camara_nunca_atraviesa_el_suelo() {
        let mut estado = Estado::nuevo();
        estado.empezar();
        // 10 minutos de vuelo a 30 fotogramas por segundo.
        for _ in 0..18_000 {
            estado.avanzar(1.0 / 30.0);
            let suelo = altura(estado.posicion.x, estado.posicion.z, estado.semilla);
            assert!(estado.posicion.y >= suelo + 0.99);
        }
    }

    /// Cuánto tarda en dibujarse un fotograma a varios tamaños:
    /// `cargo test medida_sobrevuelo -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn medida_sobrevuelo() {
        for (ancho, alto) in [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)] {
            let superficie = cairo::ImageSurface::create(cairo::Format::Rgb24, ancho, alto).unwrap();
            let cr = cairo::Context::new(&superficie).unwrap();
            let mut estado = Estado::nuevo();
            estado.empezar();
            let inicio = Instant::now();
            for _ in 0..60 {
                estado.avanzar(1.0 / 30.0);
                pintar_escena(&mut estado, &cr, ancho as f64, alto as f64, (0.0, 0.0, 0.0), (0.2, 1.0, 0.4)).unwrap();
            }
            let ms = inicio.elapsed().as_secs_f64() * 1000.0 / 60.0;
            eprintln!("{ancho}×{alto}: {ms:.1} ms por fotograma, {} triángulos", estado.triangulos.len());
        }
    }
}
