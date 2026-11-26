# RetroTerm

**[English](#english) · [Español](#español)**

**Autor / Author:** Alfonso García Tejeo <agtejeo@ugr.es>  
**Licencia / License:** [GNU GPL v3 or later](LICENSE)  
**Cambios / Changelog:** [CHANGELOG.md](CHANGELOG.md) (en español)

---

## English

A personal terminal emulator for Linux with a retro look — green, amber or
white phosphor, the VT323 font and CRT effects — built for system
administrators who spend the day jumping between servers. Written in **Rust**
with **GTK 4** and **VTE** (the terminal engine used by GNOME Terminal).

The source code is commented in Spanish on purpose: it doubles as a Rust
learning project, and the comments explain the language concepts as they
appear (borrowing, `Option`/`Result`, closures, `Rc`/`RefCell`, async, FFI…).

### Features

**Look**
- Green / amber / white phosphor, VT323 font and a blinking block cursor.
- Monochrome 256-colour palette (every colour as a shade of phosphor), or the
  programs' original colours.
- CRT effects: scan lines (adjustable intensity), vignette, rounded monitor
  frame and a **phosphor glow** around the letters (a custom GTK widget).

**Terminal**
- Tabs, with an animated hint on background tabs that receive output. SSH
  tabs are titled «profile / server»; **right-click a tab to rename it**
  (only while it is open, never saved).
- Font size with `Ctrl` `+` / `Ctrl` `-` (`Ctrl` `0` resets it).
- Terminal bell and text-field beeps are **silent by default** (a checkbox in
  Settings turns them on).
- Copy & paste (`Ctrl+Shift+C/V`, `Shift+Insert`, right-click menu, middle
  button) and optional **copy on select** to the regular clipboard.
- Optional warning before **pasting several lines into a root shell**
  (each line would run immediately): cancel, paste as one line, or paste.
- **Notepad**: a free-text side panel (`Ctrl+Shift+B`) shared by all tabs,
  to prepare or edit configurations and commands. It survives hiding it and
  switching tabs; it is **never written to disk** and is gone when you quit.

**Connections**
- Connection window with **SSH profiles** and **local profiles** (a command
  run in your shell, e.g. a VPN client), plus a fixed
  «localhost · local session» entry. The last connection used is preselected.
  Profiles can be renamed («Rename…») or copied (change the name and Save).
- Passwords stored in the **system keyring** (KWallet / GNOME Keyring through
  Secret Service) and typed automatically when ssh asks for them. One-time
  codes (OTP) are left for you to type.
- **Post-connection scripts** per profile, as `wait for => type` steps, with
  `{clave}` (the profile password) and `{root}` placeholders. For example:
  `$ => sudo -i` and `[sudo] => {clave}`.
- **KeePass integration**: unlock a `.kdbx` file with its master password and
  use its **`root` entries** as `{root}` in scripts. Only `root` entries are
  read, **only in memory**; the vault **locks itself** after 15 idle minutes
  (configurable) and whenever RetroTerm is locked.
- "Keep session open": an invisible Enter on an empty prompt after N seconds
  without typing (against servers with `TMOUT`), plus SSH keep-alives.

**Lock screen**
- Lock with the padlock button, `Ctrl+Shift+L` or automatically after N idle
  minutes. Unlock with **your Linux session password** (PAM).
- Four screensavers: a **Lissajous oscilloscope** with phosphor persistence,
  a **rain of phosphor letters**, **Invasion** (an automatic cannon against
  waves of randomly generated aliens) and **Flyover** (an endless flight over
  flat-shaded polygon terrain).
- The password box appears when you move the mouse or press a key, and hides
  again with `Esc` or after 10 s without typing.

### Keyboard shortcuts

| Keys | Action |
|---|---|
| `Ctrl+Shift+T` (or double-click on the empty tab bar) | New connection |
| `Ctrl+Shift+W` | Close tab |
| `Ctrl+Shift+C` / `Ctrl+Shift+V`, `Shift+Insert` | Copy / paste |
| `Ctrl+Shift+B` | Show / hide the notepad |
| `Ctrl+,` | Settings |
| `Ctrl` `+` / `Ctrl` `-` / `Ctrl` `0` | Larger / smaller / default font size |
| `Ctrl+Shift+L` | Lock |

### Security design

- Passwords are never written to disk or to the console. They live in the
  system keyring or, for KeePass, only in memory (root entries only).
- The KeePass master password is used once and discarded (unless you choose
  to remember it in the keyring).
- Configuration files are created readable only by you (`0600`, folder
  `0700`). They contain hosts, users and scripts, never passwords.
- Host and user names cannot start with `-`, and ssh gets `--` before the
  destination, so a crafted profile cannot inject ssh options.
- PAM is called through a small, self-contained FFI module
  (`src/pam_ffi.rs`); the copy of your password is zeroed afterwards.

### Supported systems

openSUSE Tumbleweed, Fedora, RHEL/Rocky/AlmaLinux **10**, Debian **13** and
Ubuntu **24.04** (and derivatives such as Pop!_OS 24.04 or Linux Mint 22).
VTE for GTK 4 ≥ 0.72 is required, so EL9, Debian 12 and Ubuntu 22.04 are
**not** supported.

**macOS** (Apple Silicon, tested on an M1) is supported
**experimentally**, building from source with Homebrew (see «macOS app» below). There
are no ready-made macOS packages yet. Passwords go to the macOS Keychain,
and unlocking uses your Mac password through the system PAM service `login`.
Intel Macs should work too (nothing in the code depends on the processor),
but they are **untested**, and Homebrew is phasing out Intel support, so GTK
and VTE may have to be compiled locally.

### Install a package

Ready-made packages are built with the scripts below. On the target machine:

```bash
# openSUSE
sudo zypper install ./retroterm-X.Y.Z-1.opensuse.x86_64.rpm
# Fedora / RHEL / Rocky / Alma 10
sudo dnf install ./retroterm-X.Y.Z-1.fedora.x86_64.rpm   # or .el10
# Debian 13 / Ubuntu 24.04 and derivatives
sudo apt install ./retroterm_X.Y.Z-1+ubuntu2404_amd64.deb   # or +debian13
```

The package manager pulls in GTK 4, VTE and D-Bus. Packages include the VT323
font, the icon, the menu entry and the PAM service `/etc/pam.d/retroterm`.

> **PAM tip:** the packaged PAM service includes your distribution's common
> authentication stack. If that stack uses a fingerprint reader
> (`pam_fprintd`), unlocking may wait 15–20 s for it. On a single-user
> laptop you can make `/etc/pam.d/retroterm` use only `pam_unix.so` for
> `auth` and `account`.

### Build from source

1. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. Development libraries:
   - Debian / Ubuntu / Mint / Pop!_OS: `sudo apt install build-essential pkg-config libgtk-4-dev libvte-2.91-gtk4-dev libdbus-1-dev libpam0g-dev`
   - Fedora: `sudo dnf install gcc pkgconf gtk4-devel vte291-gtk4-devel dbus-devel pam-devel`
   - openSUSE: `sudo zypper install gcc pkgconf 'pkgconfig(gtk4)' 'pkgconfig(vte-2.91-gtk4)' 'pkgconfig(dbus-1)' pam-devel`
   - Arch / Manjaro: `sudo pacman -S base-devel gtk4 vte4`
   - macOS: Xcode Command Line Tools (`xcode-select --install`) and, with
     [Homebrew](https://brew.sh), `brew install rustup vte3 pkgconf adwaita-icon-theme librsvg`, then
     `rustup default stable` (Rust can come from rustup instead of step 1;
     add `/opt/homebrew/opt/rustup/bin` to your `PATH`, or
     `/usr/local/opt/rustup/bin` on Intel Macs)
3. VT323 font (Google Fonts, OFL): copy `VT323-Regular.ttf` to
   `~/.local/share/fonts/` and run `fc-cache -f`. Without it, RetroTerm falls
   back to the default monospace font. On macOS, double-click the `.ttf` and
   install it (it goes to `~/Library/Fonts`).
4. Build and run:
   ```bash
   cargo run                 # debug build and run
   cargo build --release     # optimised binary in target/release/retroterm
   ```

No `clang` is needed: PAM is linked through RetroTerm's own FFI module.

### Building the packages

Packages are built **inside containers** of each distribution (podman or
docker), so every binary links against the libraries of the system it is
meant for. Nothing from those distributions is installed on your machine.

```bash
./empaquetado/construir-rpm.sh            # openSUSE, Fedora and EL10
./empaquetado/construir-rpm.sh fedora     # just one: opensuse | fedora | el10
./empaquetado/construir-deb.sh            # Debian 13 and Ubuntu 24.04
./empaquetado/construir-deb.sh debian13   # just one: debian13 | ubuntu2404
```

What the scripts do:
1. Read the version from `Cargo.toml`.
2. Download the VT323 font once (it goes inside the packages).
3. For each distribution, start a container, install the build tools, install
   Rust with rustup (cached in a podman volume, so later runs are faster) and
   run `cargo build --release` into its own `target-rpm-*` / `target-deb-*`
   folder.
4. Create the package: **RPM** with `rpmbuild` and `empaquetado/retroterm.spec`;
   **.deb** with `cargo-deb`, configured in `[package.metadata.deb]` in
   `Cargo.toml`.
5. Leave the results in `empaquetado/rpms/` and `empaquetado/debs/`.

The first run downloads each distribution's package metadata and can take a
while; later runs reuse the Rust toolchain and the compiled dependencies.

### macOS app

Step by step, on a Mac with no development tools (Apple Silicon; Intel
should work too, untested). It installs RetroTerm as a native app in
`~/Applications`, with its icon, ready for the Dock, Launchpad and Spotlight.

1. **Apple's command line tools** (compiler, `git`). A window opens: click
   «Install».
   ```bash
   xcode-select --install
   ```
2. **[Homebrew](https://brew.sh)**. It asks for your Mac password.
   ```bash
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   ```
   At the end, run the two lines it shows under «Next steps» (they add
   `brew` to your `PATH`).
3. **Rust, GTK 4, VTE and the rest** of the build tools:
   ```bash
   brew install rustup vte3 pkgconf adwaita-icon-theme librsvg
   rustup default stable
   ```
4. **Rust in your `PATH`** (`rustup` from Homebrew is not linked into it).
   On Intel Macs use `/usr/local/opt/rustup/bin` instead:
   ```bash
   echo 'export PATH="/opt/homebrew/opt/rustup/bin:$PATH"' >> ~/.zprofile
   source ~/.zprofile
   ```
5. **Download RetroTerm and build the app**:
   ```bash
   git clone https://github.com/agtejeo/retroterm.git
   cd retroterm
   ./empaquetado/construir-app.sh               # or: ... /Applications
   ```
6. **Open it** from Spotlight (⌘ Space, «RetroTerm»), Launchpad or Finder.
   To keep it in the Dock: right-click its icon while it is open → Options →
   Keep in Dock.

To **update** it: `git pull` and run `./empaquetado/construir-app.sh` again.

What the script does: `cargo build --release`; builds the app bundle (with
the icon made from the project's SVG via `rsvg-convert` and `iconutil`);
signs it ad hoc; installs it; and installs the VT323 font in
`~/Library/Fonts` if it is missing. The app starts RetroTerm through your
login shell, so local tabs get the same `PATH` as a normal terminal.

Good to know:
- The app uses GTK 4 and VTE **from Homebrew**; it does not carry them
  inside. Each Mac builds its own app: copying it to a Mac without those
  packages will not work.
- The first time it reads a saved password, macOS asks whether `retroterm`
  may use the Keychain item: choose «Always Allow» (it may ask again after
  each rebuild).
- Your profiles go to `~/.config/retroterm/` and your passwords to your own
  Keychain, as on Linux.

### Configuration files

`~/.config/retroterm/`:
- `ajustes.toml` — settings (font, phosphor, effects, lock, KeePass file path…).
- `perfiles.toml` — connection profiles and scripts (no passwords).

Edit them by hand only with RetroTerm closed (it rewrites them on save).

### License

RetroTerm is free software under the **GNU GPL v3 or later**: you may use,
study, modify and redistribute it; if you redistribute it (modified or not),
keep the authorship notice, use the same license and provide the source
code. The bundled VT323 font is under the SIL Open Font License 1.1.

---

## Español

Emulador de terminal personal para Linux con estética retro —fósforo verde,
ámbar o blanco, fuente VT323 y efectos CRT—, pensado para administradores de
sistemas que pasan el día saltando de servidor en servidor. Escrito en
**Rust** con **GTK 4** y **VTE** (el motor de terminal de GNOME Terminal).

El código está comentado en español a propósito: es también un proyecto para
aprender Rust, y los comentarios explican las ideas del lenguaje según
aparecen (préstamos, `Option`/`Result`, closures, `Rc`/`RefCell`, async,
FFI…).

### Funciones

**Aspecto**
- Fósforo verde / ámbar / blanco, fuente VT323 y cursor de bloque parpadeante.
- Paleta monocroma de 256 colores (todos en tonos de fósforo) o los colores
  originales de los programas.
- Efectos CRT: líneas de barrido (intensidad ajustable), viñeteado, marco de
  monitor con esquinas redondeadas y **halo del fósforo** alrededor de las
  letras (un widget propio de GTK).

**Terminal**
- Pestañas, con un aviso animado en las de segundo plano que reciben texto.
  Las de SSH se titulan «perfil / servidor»; **con el botón derecho se
  renombran** (solo mientras están abiertas, no se guarda).
- Tamaño de letra con `Ctrl` `+` / `Ctrl` `-` (`Ctrl` `0`, el de siempre).
- La campana de la terminal y los pitidos de los campos de texto, **en
  silencio por defecto** (una casilla en Ajustes los activa).
- Copiar y pegar (`Ctrl+Mayús+C/V`, `Mayús+Insert`, menú con clic derecho,
  botón central) y **copiar al seleccionar** al portapapeles normal (opcional).
- Aviso opcional antes de **pegar varias líneas en una shell de root** (cada
  línea se ejecutaría al momento): cancelar, pegar en una sola línea o pegar.
- **Bloc de notas**: panel lateral de texto libre (`Ctrl+Mayús+B`), el mismo
  para todas las pestañas, para preparar o retocar configuraciones y órdenes.
  Se conserva al ocultarlo y al cambiar de pestaña; **nunca se guarda en
  disco** y se pierde al cerrar RetroTerm.

**Conexiones**
- Ventana de conexiones con **perfiles SSH** y **perfiles locales** (una orden
  que se ejecuta en tu shell, p. ej. un cliente VPN), más la entrada fija
  «localhost · sesión local». Sale elegida la última conexión usada.
  Los perfiles se pueden renombrar («Renombrar…») o copiar (cambiar el
  nombre y Guardar).
- Contraseñas en el **llavero del sistema** (KWallet / GNOME Keyring, vía
  Secret Service), tecleadas solas cuando ssh las pide. Los códigos de un solo
  uso (OTP) los tecleas tú.
- **Guiones post-conexión** por perfil, con pasos `espera => escribe` y los
  marcadores `{clave}` (la contraseña del perfil) y `{root}`. Por ejemplo:
  `$ => sudo -i` y `[sudo] => {clave}`.
- **KeePass**: desbloquea un `.kdbx` con su contraseña maestra y usa sus
  **entradas `root`** como `{root}` en los guiones. Solo se leen las entradas
  `root`, **solo en memoria**; la bóveda **se bloquea sola** tras 15 minutos
  sin usarla (configurable) y al bloquear RetroTerm.
- «Mantener sesión abierta»: un Intro invisible en el prompt vacío tras N
  segundos sin teclear (contra servidores con `TMOUT`), más latidos de SSH.

**Bloqueo**
- Con el botón candado, `Ctrl+Mayús+L` o solo tras N minutos sin tocarlo. Se
  desbloquea con **la contraseña de tu sesión de Linux** (PAM).
- Cuatro salvapantallas: un **osciloscopio** (curvas de Lissajous) con
  persistencia de fósforo, una **lluvia de letras de fósforo**, **Invasión**
  (un cañón automático contra oleadas de alienígenas generados al azar) y
  **Sobrevuelo** (vuelo sin fin sobre un terreno de polígonos).
- La caja de la contraseña aparece al mover el ratón o pulsar una tecla, y
  se oculta con `Esc` o tras 10 s sin teclear.

### Atajos de teclado

| Teclas | Acción |
|---|---|
| `Ctrl+Mayús+T` (o doble clic en el hueco de la barra de pestañas) | Nueva conexión |
| `Ctrl+Mayús+W` | Cerrar pestaña |
| `Ctrl+Mayús+C` / `Ctrl+Mayús+V`, `Mayús+Insert` | Copiar / pegar |
| `Ctrl+Mayús+B` | Mostrar / ocultar el bloc de notas |
| `Ctrl+,` | Ajustes |
| `Ctrl` `+` / `Ctrl` `-` / `Ctrl` `0` | Letra más grande / más pequeña / normal |
| `Ctrl+Mayús+L` | Bloquear |

### Seguridad

- Las contraseñas nunca se escriben en disco ni en la consola. Están en el
  llavero del sistema o, las de KeePass, solo en memoria (solo las de root).
- La contraseña maestra de KeePass se usa una vez y se descarta (salvo que
  elijas recordarla en el llavero).
- Los ficheros de configuración solo los puedes leer tú (`0600`, carpeta
  `0700`). Guardan servidores, usuarios y guiones, nunca contraseñas.
- Servidor y usuario no pueden empezar por `-`, y ssh recibe `--` antes del
  destino: un perfil manipulado no puede colar opciones de ssh.
- PAM se usa mediante un módulo FFI propio y pequeño (`src/pam_ffi.rs`); la
  copia de tu contraseña se borra con ceros al terminar.

### Sistemas soportados

openSUSE Tumbleweed, Fedora, RHEL/Rocky/AlmaLinux **10**, Debian **13** y
Ubuntu **24.04** (y derivadas como Pop!_OS 24.04 o Linux Mint 22). Hace falta
VTE para GTK 4 ≥ 0.72, así que EL9, Debian 12 y Ubuntu 22.04 **no** están
soportados.

**macOS** (Apple Silicon, probado en un M1) funciona de forma
**experimental**, compilando desde el código con Homebrew (ver «Aplicación de macOS» más abajo).
Todavía no hay paquetes para macOS. Las contraseñas van al Llavero de macOS
y el desbloqueo usa la contraseña del Mac con el servicio PAM `login` del
sistema. En los Mac Intel debería funcionar también (nada del código depende
del procesador), pero está **sin probar**, y Homebrew va retirando el soporte
de Intel, así que puede tocar compilar GTK y VTE en el propio equipo.

### Instalar un paquete

Los paquetes se construyen con los scripts de más abajo. En el equipo de
destino:

```bash
# openSUSE
sudo zypper install ./retroterm-X.Y.Z-1.opensuse.x86_64.rpm
# Fedora / RHEL / Rocky / Alma 10
sudo dnf install ./retroterm-X.Y.Z-1.fedora.x86_64.rpm   # o .el10
# Debian 13 / Ubuntu 24.04 y derivadas
sudo apt install ./retroterm_X.Y.Z-1+ubuntu2404_amd64.deb   # o +debian13
```

El gestor de paquetes instala solo GTK 4, VTE y D-Bus. Los paquetes incluyen
la fuente VT323, el icono, la entrada del menú y el servicio PAM
`/etc/pam.d/retroterm`.

> **Consejo sobre PAM:** el servicio PAM del paquete incluye la pila de
> autenticación común de tu distribución. Si esa pila usa un lector de
> huellas (`pam_fprintd`), el desbloqueo puede esperarlo 15-20 s. En un
> portátil de un solo usuario puedes dejar `/etc/pam.d/retroterm` solo con
> `pam_unix.so` para `auth` y `account`.

### Compilar desde el código

1. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. Librerías de desarrollo:
   - Debian / Ubuntu / Mint / Pop!_OS: `sudo apt install build-essential pkg-config libgtk-4-dev libvte-2.91-gtk4-dev libdbus-1-dev libpam0g-dev`
   - Fedora: `sudo dnf install gcc pkgconf gtk4-devel vte291-gtk4-devel dbus-devel pam-devel`
   - openSUSE: `sudo zypper install gcc pkgconf 'pkgconfig(gtk4)' 'pkgconfig(vte-2.91-gtk4)' 'pkgconfig(dbus-1)' pam-devel`
   - Arch / Manjaro: `sudo pacman -S base-devel gtk4 vte4`
   - macOS: Command Line Tools de Xcode (`xcode-select --install`) y, con
     [Homebrew](https://brew.sh), `brew install rustup vte3 pkgconf adwaita-icon-theme librsvg` y luego
     `rustup default stable` (Rust puede venir de aquí en vez del paso 1;
     añade `/opt/homebrew/opt/rustup/bin` a tu `PATH`, o
     `/usr/local/opt/rustup/bin` en los Mac Intel)
3. Fuente VT323 (Google Fonts, licencia OFL): copia `VT323-Regular.ttf` a
   `~/.local/share/fonts/` y ejecuta `fc-cache -f`. Sin ella, RetroTerm usa la
   fuente monoespaciada por defecto. En macOS, haz doble clic en el `.ttf` e
   instálala (va a `~/Library/Fonts`).
4. Compilar y ejecutar:
   ```bash
   cargo run                 # compila (modo depuración) y arranca
   cargo build --release     # binario optimizado en target/release/retroterm
   ```

No hace falta `clang`: PAM se enlaza con un módulo FFI propio de RetroTerm.

### Construir los paquetes

Los paquetes se compilan **dentro de contenedores** de cada distribución
(podman o docker), para que cada binario enlace con las librerías del sistema
al que va destinado. No se instala nada de esas distribuciones en tu equipo.

```bash
./empaquetado/construir-rpm.sh            # openSUSE, Fedora y EL10
./empaquetado/construir-rpm.sh fedora     # solo uno: opensuse | fedora | el10
./empaquetado/construir-deb.sh            # Debian 13 y Ubuntu 24.04
./empaquetado/construir-deb.sh debian13   # solo uno: debian13 | ubuntu2404
```

Qué hacen los scripts:
1. Leen la versión de `Cargo.toml`.
2. Descargan una vez la fuente VT323 (va dentro de los paquetes).
3. Para cada distribución, arrancan un contenedor, instalan las herramientas
   de compilación, instalan Rust con rustup (guardado en un volumen de podman:
   las siguientes veces va más rápido) y ejecutan `cargo build --release` en
   su propia carpeta `target-rpm-*` / `target-deb-*`.
4. Crean el paquete: **RPM** con `rpmbuild` y `empaquetado/retroterm.spec`;
   **.deb** con `cargo-deb`, configurado en `[package.metadata.deb]` de
   `Cargo.toml`.
5. Dejan el resultado en `empaquetado/rpms/` y `empaquetado/debs/`.

La primera vez descargan los índices de paquetes de cada distribución y
pueden tardar; después reutilizan Rust y las dependencias ya compiladas.

### Aplicación de macOS

Paso a paso, en un Mac sin herramientas de desarrollo (Apple Silicon; en
Intel debería funcionar, sin probar). Instala RetroTerm como aplicación
nativa en `~/Applications`, con su icono, lista para el Dock, Launchpad y
Spotlight.

1. **Herramientas de línea de órdenes de Apple** (compilador, `git`). Se
   abre una ventana: pulsa «Instalar».
   ```bash
   xcode-select --install
   ```
2. **[Homebrew](https://brew.sh)**. Pide la contraseña del Mac.
   ```bash
   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
   ```
   Al terminar, ejecuta las dos líneas que muestra en «Next steps» (añaden
   `brew` a tu `PATH`).
3. **Rust, GTK 4, VTE y el resto** de herramientas para compilar:
   ```bash
   brew install rustup vte3 pkgconf adwaita-icon-theme librsvg
   rustup default stable
   ```
4. **Rust en tu `PATH`** (el `rustup` de Homebrew no se enlaza solo). En
   los Mac Intel, usa `/usr/local/opt/rustup/bin`:
   ```bash
   echo 'export PATH="/opt/homebrew/opt/rustup/bin:$PATH"' >> ~/.zprofile
   source ~/.zprofile
   ```
5. **Descargar RetroTerm y construir la app**:
   ```bash
   git clone https://github.com/agtejeo/retroterm.git
   cd retroterm
   ./empaquetado/construir-app.sh               # o: ... /Applications
   ```
6. **Ábrela** desde Spotlight (⌘ espacio, «RetroTerm»), Launchpad o el
   Finder. Para dejarla en el Dock: clic derecho en su icono mientras está
   abierta → Opciones → Mantener en el Dock.

Para **actualizarla**: `git pull` y vuelve a ejecutar
`./empaquetado/construir-app.sh`.

Qué hace el script: `cargo build --release`; monta la app (con el icono hecho
a partir del SVG del proyecto con `rsvg-convert` e `iconutil`); la firma ad
hoc; la instala; e instala la fuente VT323 en `~/Library/Fonts` si falta. La
app arranca RetroTerm a través de tu shell de inicio de sesión, así que las
pestañas locales tienen el mismo `PATH` que una terminal normal.

Conviene saber:
- La app usa GTK 4 y VTE **de Homebrew**; no los lleva dentro. Cada Mac
  construye su propia app: copiarla a un Mac sin esos paquetes no funciona.
- La primera vez que lea una contraseña guardada, macOS preguntará si
  `retroterm` puede usar ese elemento del Llavero: elige «Permitir siempre»
  (puede volver a preguntar tras recompilar).
- Tus perfiles van a `~/.config/retroterm/` y tus contraseñas a tu propio
  Llavero, igual que en Linux.

### Ficheros de configuración

`~/.config/retroterm/`:
- `ajustes.toml`: ajustes (fuente, fósforo, efectos, bloqueo, ruta del
  fichero KeePass…).
- `perfiles.toml`: perfiles de conexión y guiones (sin contraseñas).

Edítalos a mano solo con RetroTerm cerrado (al guardar, los reescribe).

### Licencia

RetroTerm es software libre bajo la **GNU GPL v3 o posterior**: puedes
usarlo, estudiarlo, modificarlo y redistribuirlo; si lo redistribuyes
(modificado o no), debes mantener el aviso de autoría, usar la misma licencia
y ofrecer el código fuente. La fuente VT323 incluida tiene licencia SIL Open
Font License 1.1.

---

## Hoja de ruta / Roadmap

- [x] Fase 1: terminal con fuente, colores de fósforo y cursor de bloque parpadeante
- [x] Fase 2: ventana de inicio con perfiles de servidor (guardados en ~/.config/retroterm/perfiles.toml)
- [x] Fase 3: contraseña en el llavero del sistema (KWallet / GNOME Keyring) y escrita automáticamente al ver `password:`
- [x] Fase 4: guion por perfil (`espera => escribe`, con `{clave}` para la contraseña), p. ej. `$ => sudo -i` y `[sudo] => {clave}`
- [x] Paleta monocroma (los 256 colores en tonos de fósforo)
- [x] Pestañas (Ctrl+Mayús+T nueva, Ctrl+Mayús+W cerrar); el último `exit` cierra la ventana y termina el programa
- [x] Copiar y pegar: Ctrl+Mayús+C / Ctrl+Mayús+V (o Mayús+Insert), menú con clic derecho, botón central
- [x] Efectos CRT: líneas de barrido, viñeteado y esquinas redondeadas (crt.rs)
- [x] Ventana de ajustes (Ctrl+, · botón ⚙ · clic derecho): fuente, tamaño, fósforo verde/ámbar/blanco, paleta o colores originales, efectos CRT; se guarda en ~/.config/retroterm/ajustes.toml
- [x] "Mantener sesión abierta" por perfil: Intro en prompt vacío tras N s sin teclear (contra TMOUT) + ssh -o ServerAliveInterval=60; intervalo en Ajustes → Sesiones
- [x] Autoría y licencia GPLv3, CHANGELOG, ventana "Acerca de"
- [x] Halo del fósforo con un widget propio (halo.rs), con intensidad ajustable
- [x] KeePass: desbloqueo con contraseña maestra (solo en memoria), entradas de usuario root en el desplegable "Root (KeePass)" y `{root}` en el guion (p. ej. `$ => su -` y `Password:|Contraseña: => {root}`)
- [x] "Aplicar cambios a caracteres" (por perfil, marcada por defecto) a la contraseña de KeePass; reglas en boveda::CAMBIOS
- [x] Bloqueo automático de KeePass por inactividad (15 min por defecto) y al bloquear RetroTerm
- [x] Bloc de notas: panel de texto libre a la derecha (Ctrl+Mayús+B), compartido por todas las pestañas, solo en memoria
- [ ] Bloc de notas: botón «Enviar a la terminal»
- [x] Retoques CRT: deslizador de intensidad de las líneas, marco de monitor con esquinas redondeadas; estilo del selector de tamaño
- [x] Aviso opcional al pegar varias líneas en una shell de root (pegado.rs)
- [x] Copiar al seleccionar (al portapapeles normal), con casilla en Ajustes
- [x] Perfiles locales con «Comando local» (p. ej. la VPN con openconnect) y guion como los de SSH
- [x] Ventana de conexiones: entrada fija «localhost · sesión local» y se recuerda la última conexión usada
- [x] Salvapantallas de bloqueo a elegir en Ajustes → Bloqueo: osciloscopio (Lissajous) o lluvia de letras de fósforo
- [x] PAM con FFI propio (sin clang para compilar)
- [x] Ventana de conexiones: entrada fija «+ Perfil nuevo» (sin perfiles no se podía crear el primero)
- [x] Tamaño de letra con Ctrl++ / Ctrl+- (y Ctrl+0 para el tamaño por defecto)
- [x] Salvapantallas «Invasión»: cañón automático contra oleadas de alienígenas generados al azar
- [x] Salvapantallas «Sobrevuelo»: terreno poligonal infinito con sombreado plano, niebla, lagos y curvas
- [x] Caja de contraseña del bloqueo: se oculta con Esc o tras 10 s sin teclear
- [x] Sonidos de aviso (campana de la terminal y campos de texto) activables en Ajustes; en silencio por defecto
- [x] Al cerrar una pestaña, la que queda activa recibe el foco del teclado
- [x] Renombrar pestañas con el botón derecho (solo mientras están abiertas)
- [x] Pestañas SSH con «perfil / servidor»
- [x] Renombrar perfiles (botón «Renombrar…» en la ventana de conexiones)
- [x] macOS (experimental): compila con Homebrew; contraseñas en el Llavero de macOS y desbloqueo con el PAM del sistema (OpenPAM)
- [x] macOS: `RetroTerm.app` para el propio Mac (`empaquetado/construir-app.sh`)
- [ ] macOS: atajos con ⌘ y una `.app` autónoma (con GTK y VTE dentro)
