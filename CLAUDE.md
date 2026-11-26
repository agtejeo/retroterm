# CLAUDE.md — RetroTerm

Instrucciones permanentes para Claude Code en este proyecto. Se cargan al
empezar cada sesión. Mantenlas al día cuando cambie algo importante.

## Qué es

RetroTerm: emulador de terminal personal para Linux (y, de forma
experimental desde la 0.28.0, macOS) con estética retro
(fósforo verde/ámbar/blanco, fuente VT323, efectos CRT), en **Rust + GTK 4 +
VTE**. Perfiles SSH, contraseñas en el llavero del sistema (KWallet),
guiones post-conexión ("espera => escribe"), contraseñas de root desde un
fichero KeePass, pestañas, bloqueo con salvapantallas (osciloscopio, lluvia,
invasión, sobrevuelo) y
desbloqueo con la contraseña de sesión (PAM).

- **Autor:** Alfonso García Tejeo <agtejeo@ugr.es>
- **Licencia:** GPL-3.0-or-later (fichero `LICENSE`). Proyecto personal,
  hecho en equipo propio y tiempo libre.
- **Historial:** `CHANGELOG.md`. Hoja de ruta: `README.md`.

## Cómo trabajar con Alfonso (IMPORTANTE)

- **Responde siempre en español.** Alfonso es administrador de sistemas con
  experiencia, pero **está aprendiendo Rust con este proyecto**.
- **El código es didáctico:** los comentarios, en español, explican la
  sintaxis y las ideas de Rust que aparecen (préstamos, Option/Result, `?`,
  closures y `move`, Rc/RefCell/Cell, referencias débiles, traits, async…).
  Al introducir algo nuevo, coméntalo en el código y explícalo brevemente.
- Identificadores en español **sin tildes ni ñ** (`contrasena` no, mejor
  `clave`; `pestana`, `vineta`, `tamano`). Los textos de la interfaz sí
  llevan tildes.
- Cambios pequeños y verificables. Antes de refactorizaciones grandes,
  explica el plan y espera su visto bueno.
- Tras cada cambio que funcione: sugiere hacer **commit en git**.

## Reglas de versionado (en CADA cambio que se entregue)

1. Subir `version` en `Cargo.toml` (0.x.y: x para funciones nuevas, y para
   correcciones).
2. Añadir la sección de la versión **arriba del todo** en `CHANGELOG.md`
   (Añadido / Cambiado / Corregido), con la fecha. **Desde la 0.27.0, en
   español y en inglés** (Added / Changed / Fixed), y las notas de la
   release de GitHub, también bilingües.
3. Si es una función nueva, marcarla en la hoja de ruta del `README.md`.
4. Cada fichero `.rs` nuevo lleva la cabecera de licencia:
   `// SPDX-License-Identifier: GPL-3.0-or-later` +
   `// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>` + aviso GPL
   (copiarla de cualquier otro fichero de `src/`).

## Compilar y probar

```bash
cargo build          # compilar
cargo run            # compilar y ejecutar (la consola muestra los avisos)
```

Dependencias del sistema (openSUSE Tumbleweed, el equipo de Alfonso):
`gcc pkgconf 'pkgconfig(gtk4)' 'pkgconfig(vte-2.91-gtk4)' 'pkgconfig(dbus-1)' pam-devel`
(pam-devel para enlazar con libpam; ya no hace falta clang).

macOS (Apple Silicon): Command Line Tools de Xcode y, con Homebrew,
`brew install rustup vte3 pkgconf adwaita-icon-theme` + `rustup default stable`.
`rustup` es keg-only: hay que tener `/opt/homebrew/opt/rustup/bin` en el
`PATH`. Sin `adwaita-icon-theme`, los iconos de los botones (`*-symbolic`)
salen como «imagen que falta»: Homebrew solo trae el tema `hicolor`.

Después de tocar código, **compila siempre** y corrige los errores antes de
dar nada por terminado. Las pruebas de interfaz las hace Alfonso a mano:
dile exactamente qué probar.

## Versiones fijadas (NO cambiar sin motivo)

- `vte4 = { version = "0.9", features = ["v0_72"] }`: la 0.10 no compila con
  gtk4 0.11.5. `v0_72` hace falta para leer el texto de la pantalla.
- `keepass = "=0.8.12"` (exacta): la 0.8.21 quitó `Node` y `Group.children`.
- `keyring = "3"` va POR SISTEMA (`[target.'cfg(target_os = ...)'.dependencies]`):
  Linux `["sync-secret-service", "crypto-rust"]`, macOS `["apple-native"]`.
  Si cambias la versión, cámbiala en las dos secciones.
- PAM sin crate: FFI propio en `pam_ffi.rs` (solo `libc = "0.2"`). Todo el
  `unsafe` del programa vive ahí; hacia fuera solo `autenticar()`.
- `[profile.dev.package."*"] opt-level = 3`: sin esto, descifrar el .kdbx
  (Argon2) tarda decenas de segundos en modo depuración.
- `[profile.dev] opt-level = 1` (nuestro código): sin esto, los
  salvapantallas a pantalla completa van ~10 veces más lentos con `cargo run`.
- GTK se usa a través de `vte4::gtk` (no hay dependencia directa de gtk4).

## Mapa de módulos (`src/`)

| Módulo | Qué hace |
|---|---|
| `main.rs` | Arranque: app GTK, estilos, ventana de conexiones |
| `lanzador.rs` | Ventana de conexiones: perfiles, contraseña, KeePass, guion |
| `ventana_terminal.rs` | Ventana de terminales: pestañas, acciones, atajos, menú |
| `perfil.rs` | Perfiles (SSH, o locales con `comando_local`) y guion; `~/.config/retroterm/perfiles.toml` |
| `ajustes.rs` | Ajustes globales (thread_local) en `ajustes.toml` |
| `preferencias.rs` | Ventana de ajustes |
| `tema.rs` | Colores de fósforo, aplicar a la terminal, **CSS** |
| `paleta.rs` | Paleta monocroma de 256 colores |
| `crt.rs` | Capa CRT sobre cada terminal (líneas, viñeteado, marco); `terminal_de`/`pagina_de` |
| `halo.rs` | Widget propio: halo del fósforo (terminal desenfocada + mezcla Screen, GSK) |
| `llavero.rs` | Secret Service (KWallet) o Llavero de macOS: claves de perfiles y maestra de KeePass |
| `boveda.rs` | KeePass: solo entradas `root`, solo en memoria; `CAMBIOS`; bloqueo automático (temporizador) |
| `desbloqueo.rs` | Desbloqueo de KeePass (async, en otro hilo) |
| `autorespuesta.rs` | "Espera => escribe": lee la línea del cursor y teclea |
| `bloc.rs` | Bloc de notas: panel de texto a la derecha; `TextBuffer` global, nunca a disco |
| `pegado.rs` | Aviso al pegar varias líneas en un prompt de root (opcional) |
| `mantener.rs` | "Mantener sesión abierta" (Intro en prompt vacío contra TMOUT) |
| `actividad.rs` | Aviso animado en pestañas en segundo plano con novedades |
| `bloqueo.rs` | Bloqueo: capa con salvapantallas, teclado/ratón, autobloqueo |
| `salvapantallas.rs` | Elige el efecto (gtk::Stack); `Lienzo` con estela y limpieza de restos; rótulo |
| `osciloscopio.rs` | Salvapantallas: Lissajous con persistencia (Cairo) |
| `lluvia.rs` | Salvapantallas: lluvia de letras de fósforo |
| `invasion.rs` | Salvapantallas: cañón automático contra oleadas de alienígenas al azar; constantes de ajuste arriba; `cargo test invasion -- --nocapture` simula el equilibrio |
| `sobrevuelo.rs` | Salvapantallas: vuelo sobre terreno de polígonos (ruido de valores, `Vec3` con operadores, algoritmo del pintor, sin antialias por rendimiento) |
| `sesion.rs` | Comprobar la contraseña de sesión con PAM (servicios `retroterm`, `login`) |
| `pam_ffi.rs` | FFI propio con libpam (`unsafe`, conversación, `Drop` → `pam_end`) |
| `acerca.rs` | Diálogo "Acerca de" (versión y autor salen de Cargo.toml) |

## Estructura de widgets (para no romperla)

```
Ventana de terminales (ApplicationWindow, clase CSS "terminales")
└─ raíz: gtk::Overlay
   ├─ gtk::Paned                          ← ventana_terminal::contenido_de()
   │  ├─ gtk::Notebook (pestañas)         ← ventana_terminal::cuaderno_de()
   │  │  └─ cada página: gtk::Overlay     ← crt::envolver(), crt::pagina_de()
   │  │     ├─ Halo (widget propio, halo.rs)
   │  │     │  └─ vte4::Terminal          ← crt::terminal_de(), terminal_activa()
   │  │     └─ DrawingArea (capa CRT, no recibe ratón)
   │  └─ bloc de notas (Box, clase "bloc", oculto; bloc.rs)
   └─ capa de bloqueo (Overlay, clase "bloqueo", oculta)
      ├─ salvapantallas (gtk::Stack: osciloscopio | lluvia | invasion | sobrevuelo, DrawingArea)
      └─ caja de contraseña (clase "caja-clave")
```
Las otras ventanas (conexiones, ajustes) llevan la clase CSS "retro".
La terminal NO es hija directa de la página: para ir de una a otra usa
siempre `crt::terminal_de()` y `crt::pagina_de()`, nunca `parent()`/`child()`.

## Trampas conocidas (ya nos han mordido)

- **CSS de `tema.rs`:** está dentro de un `format!`. Las llaves del CSS van
  dobles `{{ }}` y **NUNCA comillas dobles `"` sin escapar**, ni siquiera en
  comentarios CSS (usa « »). Para la fuente: `\"{fuente}\"`.
- **RefCell:** no tengas un `borrow_mut()` vivo mientras llamas a algo de
  GTK que puede emitir señales (p. ej. `set_model`, que dispara
  `selected_notify`). Suelta el préstamo antes (bloque `{ }`).
- **Closures guardados en widgets:** si capturan la ventana o el propio
  widget, usa referencias débiles (`downgrade()`/`upgrade()`).
- **Métodos ambiguos de traits:** `ventana.set_focus(...)` y `ventana.focus()`
  existen en `GtkWindowExt` y en `RootExt`: llama a
  `GtkWindowExt::set_focus(ventana, ...)` / `GtkWindowExt::focus(ventana)`.
- **PAM:** se usa SOLO el primero que exista de `retroterm`, `login` (probar
  `login` tras un rechazo hacía esperar ~30 s por `pam_fprintd`). **No usar
  `kde`** (rechaza a otros programas). Las pilas comunes pueden incluir
  `pam_fprintd` (el lector de huellas) y hacer esperar 15-20 s; con solo
  `pam_unix` no hay espera. Aun así, los paquetes siguen con la pila común
  (decidido el 2026-11-25): con solo `pam_unix`, quien tenga cuentas
  LDAP/SSSD no podría desbloquear. El README explica cómo quitar la espera.
- **Linux-PAM y OpenPAM (macOS)** comparten funciones y estructuras, pero
  NO todos los códigos de error: `PAM_CONV_ERR` es 19 / 6. Cualquier
  constante nueva en `pam_ffi.rs`: comprobar su valor en los dos.
- **Seguridad:** nunca escribir contraseñas en disco ni en la consola
  (`eprintln!`). KeePass: solo entradas `root`, solo en memoria.
  Ficheros de configuración: siempre con `ajustes::escribir_privado` (0600,
  carpeta 0700), nunca `fs::write`. Lo que vaya a la línea de órdenes de
  ssh: `--` antes del destino y `perfil::dato_ssh_valido` (nada que empiece
  por `-`).
- **Avisos «GtkGizmo (slider)»:** el «slider» puede ser el del deslizador
  o el de una barra de desplazamiento (Breeze les da márgenes negativos).
  Ambos tienen CSS propio en `tema.rs`. El aviso `Gdk-WARNING … Duplicate
  call to gdk_frame_timings_presented()` es de GTK/KWin, no nuestro.
- **Ventana de Ajustes:** no se puede reescalar; sus secciones van en un
  `ScrolledWindow` para que quepa en pantallas bajas. Si crece, no pasa nada.
- **Editar `perfiles.toml` o `ajustes.toml` a mano:** solo con RetroTerm
  cerrado; si no, al guardar reescribe su copia en memoria y se pierde el
  cambio. Haz copia antes.
- `mantener.rs` llama a `actividad::silenciar` antes de su Intro, para no
  encender el aviso de actividad.

## Empaquetado (`empaquetado/`)

- `construir-rpm.sh [opensuse|fedora|el10]` y `construir-deb.sh
  [debian13|ubuntu2404]`: compilan en contenedores (podman) de cada distro.
- NO soportados (VTE para GTK 4 < 0.72): EL9, Debian 12, Ubuntu 22.04.
- `retroterm.spec`: en `%install` solo órdenes `install`; en `%files` solo
  rutas. (Un fallo de mezcla ya ocurrió en 0.14.0.)
- El `.deb` se configura en `[package.metadata.deb]` de `Cargo.toml`.
- Servicios PAM por familia: `empaquetado/pam/retroterm-{suse,redhat,debian}`.
- Al añadir dependencias de compilación, actualiza los dos scripts y el README.

## Entorno de Alfonso (contexto útil)

- openSUSE Tumbleweed, KDE Plasma 6, KWallet como Secret Service.
- Un Mac con Apple Silicon para el port a macOS.
- Servidores con contraseña + código OTP (el OTP lo teclea él).
- Algunos servidores de otros equipos tienen `TMOUT=900`.
- KeePass: grupos por servicio; una entrada `root` puede servir para una
  granja de clones (p. ej. `WEB / NODOS` → web01…web19).

## Pendiente

- Bloc de notas: botón «Enviar a la terminal» (pegar la selección o todo en
  la pestaña activa, pasando por `pegado::pegar`).
- macOS: atajos con ⌘ además de Ctrl, y empaquetar un `.app`.
