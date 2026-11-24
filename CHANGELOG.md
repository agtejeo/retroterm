# Historial de cambios de RetroTerm

Autor: **Alfonso García Tejeo** <agtejeo@ugr.es>
Licencia: GNU GPL versión 3 o posterior (ver `LICENSE`).

El formato sigue la idea de [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/)
y las versiones, el [versionado semántico](https://semver.org/lang/es/):
mientras la versión sea 0.x, cada versión "menor" (0.**x**.0) añade funciones.
La versión de verdad está en `Cargo.toml`; al publicar una nueva, se añade su
sección aquí arriba del todo.

## [0.26.2] - 2026-11-23
### Corregido
- Empaquetado RPM: `retroterm.spec` tenía cinco rutas de `%files`
  (LICENSE, CHANGELOG.md y el servicio PAM) metidas en `%install`, y
  `rpmbuild` intentaba ejecutarlas como órdenes. Fallaban los RPM de
  Fedora y EL10 (el de openSUSE no llegó a probarse).

## [0.26.1] - 2026-11-23
### Corregido (seguridad)
- Un perfil SSH con servidor o usuario que empezara por `-` (p. ej.
  `-oProxyCommand=…`, en un perfiles.toml ajeno o editado a mano) se
  convertía en una OPCIÓN de ssh, y algunas ejecutan órdenes en el equipo
  local. Ahora ssh recibe `--` antes del destino, y el lanzador rechaza
  servidores y usuarios con espacios o que empiecen por `-`.
- `ajustes.toml` y `perfiles.toml` se guardaban con permisos 0644 (legibles
  por otros usuarios si /home lo es, como en Debian/Ubuntu). Ahora 0600, y
  la carpeta ~/.config/retroterm 0700; los existentes se corrigen al guardar.

### Cambiado
- Ejemplos de los comentarios con nombres de usuario genéricos.

## [0.26.0] - 2026-11-23
### Añadido
- Bloc de notas: panel de texto libre a la derecha de las terminales, con
  el botón 📝 o Ctrl+Mayús+B. Es el mismo para todas las pestañas; al
  ocultarlo no se pierde nada (solo al cerrar RetroTerm). Al abrirse ocupa
  1/3 del ancho, maximizada la ventana o no; si arrastras el divisor, se
  recuerda tu proporción (entre el 15 % y el 75 %). Nunca se guarda en disco.
  Al abrirlo y al cerrarlo, el foco se queda en la terminal; para escribir
  en el bloc, un clic en él.
- Con el cursor en el bloc, Ctrl+Mayús+C/V y Mayús+Insert copian y pegan
  en él (Ctrl+C/V, Ctrl+Z para deshacer, etc. funcionan como siempre).

### Cambiado
- La ventana de terminales tiene ahora un `gtk::Paned` (cuaderno | bloc)
  bajo la capa de bloqueo, que tapa y desactiva también el bloc.

## [0.25.1] - 2026-11-20
### Cambiado
- La contraseña de sesión se comprueba con una conexión propia con libpam
  (`src/pam_ffi.rs`) en lugar del crate `pam`. Ya no hace falta clang
  (`clang-devel` / `libclang-dev`) para compilar ni para empaquetar.
- La copia de la contraseña que usa RetroTerm para PAM se borra de la
  memoria (se sobrescribe con ceros) al terminar.

## [0.25.0] - 2026-11-20
### Añadido
- Bloqueo automático de KeePass: tras 15 minutos (ajustable en Ajustes →
  KeePass; 0 = nunca) sin usar ninguna contraseña de root, la bóveda se
  bloquea sola y las borra de la memoria. El plazo vuelve a empezar cada vez
  que un guion usa `{root}`.
- KeePass se bloquea también al bloquear RetroTerm (candado o autobloqueo).
- La ventana de conexiones, si está abierta, refleja el bloqueo al momento.

## [0.24.0] - 2026-11-19
### Añadido
- Aviso al pegar varias líneas en una shell de root (opcional, desactivado
  por defecto: Ajustes → Sesiones). Si el cursor está en un prompt que acaba
  en `#` y el texto lleva saltos de línea, se pregunta antes: «Cancelar»
  (por defecto), «Pegar en una sola línea» o «Pegar». Vale para
  Ctrl+Mayús+V, Mayús+Insert, el menú «Pegar» y el botón central.

## [0.23.0] - 2026-11-19
### Añadido
- Halo del fósforo: resplandor suave alrededor de las letras, como en un
  monitor CRT. Lo pinta un widget propio (`halo.rs`) que dibuja la terminal
  dos veces, la segunda desenfocada y mezclada como luz (GSK, en la tarjeta
  gráfica). Ajustes → Pantalla CRT: casilla «Halo del fósforo» (activada por
  defecto) e «Intensidad del halo».

### Cambiado
- Estructura de cada pestaña: página (Overlay) → Halo → Terminal. Para ir de
  la página a la terminal y al revés están `crt::terminal_de()` y
  `crt::pagina_de()`.

## [0.22.0] - 2026-11-19
### Añadido
- «Copiar al seleccionar»: al seleccionar texto en una terminal se copia
  también al portapapeles normal (Ctrl+V en otros programas), tras una
  pausa de 300 ms para no copiar trozos a medias mientras arrastras.
  Casilla en Ajustes → Texto, activada por defecto.

## [0.21.1] - 2026-11-19
### Cambiado
- No se puede guardar un perfil SSH llamado «localhost» (en mayúsculas o
  minúsculas): se confundía con la entrada «localhost · sesión local». El
  servidor sí puede ser localhost, con otro nombre de perfil.

## [0.21.0] - 2026-11-19
### Añadido
- Perfiles locales con comando: campo «Comando local» en la ventana de
  conexiones. Si tiene algo, el perfil no usa ssh: abre tu shell en una
  pestaña nueva, teclea el comando y después actúa el guion (con `{clave}`
  y `{root}` como siempre). Útil para la VPN (`sudo openconnect …`).
- La contraseña de un perfil local se guarda en el llavero con la cuenta
  `local:<nombre del perfil>`.

## [0.20.0] - 2026-11-19
### Añadido
- Ventana de conexiones: entrada fija «localhost · sesión local» al final
  del desplegable de perfiles. Al elegirla, el formulario se desactiva y
  «Conectar» abre tu shell en una pestaña nueva, sin usuario ni contraseña.
- Al abrir la ventana de conexiones sale elegida la última conexión usada
  (un perfil o la sesión local). Se guarda en `ajustes.toml`
  (`ultima_conexion`).

## [0.19.2] - 2026-11-19
### Corregido
- Avisos "Trying to measure GtkWindow for height of 1040, but it needs at
  least 1059" al abrir Ajustes: la ventana ya no cabía en pantallas de poca
  altura. Las secciones van ahora en una zona desplazable y los botones
  quedan fijos abajo.

## [0.19.1] - 2026-11-19
### Añadido
- Ventana de Ajustes: botón «Aceptar» para cerrarla (los cambios se siguen
  guardando y aplicando al momento).

## [0.19.0] - 2026-11-19
### Añadido
- Nuevo salvapantallas «Lluvia de letras»: columnas de caracteres que caen
  con la cabeza brillante y estela de fósforo. Se elige en Ajustes →
  Bloqueo → Salvapantallas (osciloscopio o lluvia) y se recuerda en
  `ajustes.toml` (`salvapantallas = "lluvia"`).

### Cambiado
- Lo común a los salvapantallas (lienzo con estela, limpieza de restos,
  líneas de barrido y rótulo con la hora) pasa a `salvapantallas.rs`.

## [0.18.9] - 2026-11-19
### Corregido
- Avisos "GtkGizmo (slider) reported min width/height -2" en la consola.
  No venían del deslizador de intensidad sino de las barras de
  desplazamiento: el tema Breeze les da márgenes negativos que dejaban su
  tamaño en -2 píxeles. Ahora tienen estilo propio (color de fósforo).

## [0.18.8] - 2026-11-19
### Cambiado
- Si una conexión SSH falla (contraseña rechazada, Ctrl+C, servidor
  inalcanzable...) y era la última pestaña, al pulsar una tecla se vuelve a
  la ventana de conexiones en lugar de cerrar RetroTerm.

## [0.18.7] - 2026-11-19
### Cambiado
- Ventana de conexiones: tras elegir un perfil en el desplegable, el foco
  vuelve al botón «Conectar», así que Intro conecta con ese perfil.

## [0.18.6] - 2026-11-19
### Cambiado
- Ventana de conexiones: al abrirse, el foco está en el botón «Conectar»,
  así que basta con pulsar Intro para conectar con el perfil elegido.

## [0.18.5] - 2026-11-19
### Cambiado
- Salvapantallas (osciloscopio): las curvas ocupan toda la pantalla (antes,
  solo el 76 % del ancho y el 72 % del alto) y se adaptan al reescalar la
  ventana. El rótulo con la hora lleva un recuadro del color de fondo para
  que se lea aunque la curva pase por detrás.

## [0.18.4] - 2026-11-19
### Corregido
- Salvapantallas (osciloscopio): la estela no terminaba de apagarse y la
  pantalla se iba llenando de restos tenues (el oscurecido gradual se
  atascaba por el redondeo a 8 bits). Ahora una limpieza por franjas acerca
  cada píxel al fondo de 1 en 1 hasta que desaparece.
- Si cambia el tema con el salvapantallas en marcha, el lienzo se rehace
  con el nuevo color de fondo.

## [0.18.3] - 2026-11-19
### Corregido
- Conectar con un perfil que usa `{root}` y KeePass bloqueado pedía dos
  clics (uno para desbloquear y otro para conectar). Ahora, al desbloquear,
  la conexión sigue sola.

## [0.18.2] - 2026-11-19
### Corregido
- Desbloqueo casi 20 s más lento de lo necesario: ya no se prueba el servicio
  PAM "kde" (rechaza a otros programas y cada rechazo añade una espera de
  seguridad). Ahora se usa "retroterm" (paquetes) o "login" (`cargo run`).
- Avisos "GtkGizmo (slider) reported min width/height -2" en la consola.

## [0.18.1] - 2026-11-19
### Corregido
- Desbloqueo: se prueban en orden todos los servicios PAM disponibles
  (retroterm, kde, login) y cada intento se anota en la consola con el
  error real de PAM, para poder diagnosticar fallos.
- Ambigüedad de `set_focus` que impedía compilar la 0.18.0.

## [0.18.0] - 2026-11-19
### Añadido
- Bloqueo con salvapantallas: botón candado (a la derecha del ⚙), Ctrl+Mayús+L
  o clic derecho → "Bloquear". Tapa la ventana de terminales (pestañas
  incluidas) con un osciloscopio: curvas de Lissajous con la persistencia del
  fósforo, y la hora. Al mover el ratón o pulsar una tecla pide la contraseña
  de tu sesión de Linux, comprobada con PAM. Las sesiones siguen vivas.
- Bloqueo automático tras N minutos sin tocar RetroTerm (0 = nunca, por
  defecto) y casilla para desactivar el bloqueo, en Ajustes → Bloqueo.
- Servicio PAM `/etc/pam.d/retroterm` en los paquetes RPM y .deb.
### Cambiado
- Nuevas dependencias de compilación: pam-devel / libpam0g-dev y
  clang-devel / libclang-dev (solo para compilar, no para ejecutar).

## [0.17.0] - 2026-11-19
### Añadido
- Aviso de actividad: si una pestaña en segundo plano recibe texto, su
  etiqueta se pone en negrita con un barrido de luz animado (de izquierda a
  derecha) hasta que la abres. El Intro de "Mantener sesión abierta" no
  cuenta. Se puede desactivar en Ajustes → Sesiones.

## [0.16.0] - 2026-11-19
### Añadido
- Doble clic en el hueco vacío de la barra de pestañas: abre la ventana de
  conexiones (como el botón + o Ctrl+Mayús+T).
### Corregido
- Comillas dobles sin escapar en un comentario del CSS que impedían compilar
  la 0.15.0.

## [0.15.0] - 2026-11-19
### Añadido
- Casilla "Recordar contraseña maestra en el llavero" (desmarcada por
  defecto) al desbloquear KeePass. Si está guardada, KeePass se abre sin
  preguntar; si deja de servir, se vuelve a pedir.
- Botón "Olvidar contraseña maestra guardada" en Ajustes → KeePass.
### Corregido
- Listas de los desplegables (perfiles, KeePass, fuentes) con fondo blanco
  y texto verde, casi ilegibles: ahora usan los colores del fósforo.
- Avisos "GtkGizmo (slider) reported min height…" en la consola.

## [0.14.0] - 2026-11-18
### Añadido
- Autoría y licencia: RetroTerm se publica bajo la GNU GPL v3 o posterior.
  Fichero `LICENSE` con el texto oficial y cabecera con autor y licencia en
  cada fichero de código.
- Este `CHANGELOG.md`, que se instala también con los paquetes RPM y .deb.
- Ventana "Acerca de RetroTerm" (clic derecho o Ajustes): versión, autor,
  licencia y créditos.
### Cambiado
- Los paquetes RPM y .deb declaran autor y licencia GPL-3.0-or-later.

## [0.13.0] - 2026-11-18
### Añadido
- "Mantener sesión abierta" (por perfil): tras N segundos sin teclear, si el
  cursor está en un prompt vacío, envía un Intro para que no salte `TMOUT`.
  Además lanza ssh con `ServerAliveInterval=60`. Intervalo configurable en
  Ajustes → Sesiones (300 s por defecto).

## [0.12.0] - 2026-11-18
### Añadido
- Deslizador "Intensidad" de las líneas de barrido.
- Marco de monitor (bisel) con esquinas redondeadas y brillo del fósforo.
- Icono propio, entrada en el menú de aplicaciones y empaquetado: RPM para
  openSUSE, Fedora y EL10 (Rocky/Alma 10), y .deb para Debian 13 y Ubuntu
  24.04, compilados en contenedores con podman.
### Corregido
- Líneas de barrido y esquinas apenas visibles sobre fondo oscuro.
- Estilo del selector de tamaño y del desplegable de fuente.

## [0.11.0] - 2026-11-18
### Añadido
- Casilla "Aplicar cambios a caracteres" (por perfil, marcada por defecto)
  que transforma la contraseña de KeePass antes de escribirla.

## [0.10.0] - 2026-11-18
### Añadido
- Integración con KeePass: desbloqueo del `.kdbx` con contraseña maestra (en
  segundo plano), entradas de usuario `root` en el desplegable "Root
  (KeePass)", marcador `{root}` en los guiones. Nada descifrado se escribe en
  disco; "Bloquear" lo borra de la memoria.
- Varias esperas alternativas en un paso del guion: `Password:|Contraseña:`.
### Corregido
- La librería `keepass` se fija en `=0.8.12` (la 0.8.21 cambió su estructura).

## [0.9.0] - 2026-11-18
### Añadido
- Ventana de ajustes (Ctrl+, · botón ⚙ · clic derecho): fuente, tamaño,
  fósforo verde/ámbar/blanco, paleta de fósforo o colores originales, y
  efectos CRT. Se aplican al momento y se guardan en
  `~/.config/retroterm/ajustes.toml`.

## [0.8.0] - 2026-11-18
### Añadido
- Efectos CRT: líneas de barrido, viñeteado y esquinas redondeadas.

## [0.7.0] - 2026-11-18
### Añadido
- Copiar y pegar: Ctrl+Mayús+C / Ctrl+Mayús+V (o Mayús+Insert) y menú con
  clic derecho.
### Cambiado
- Los atajos se capturan en la ventana antes de que lleguen a la terminal.

## [0.6.0] - 2026-11-18
### Añadido
- Pestañas en una única ventana de terminales; botón "+" y Ctrl+Mayús+T para
  nuevas conexiones, Ctrl+Mayús+W para cerrar.
- Al terminar la última sesión se cierra la ventana y termina el programa.
- Si ssh falla, la pestaña muestra "CONEXION FALLIDA" y espera una tecla.

## [0.5.0] - 2026-11-18
### Añadido
- Paleta monocroma: los 256 colores de terminal convertidos a tonos del fósforo.

## [0.4.0] - 2026-11-18
### Añadido
- Guion post-conexión por perfil (`espera => escribe`, con `{clave}`), p. ej.
  `$ => sudo -i` y `[sudo] => {clave}`.
- La contraseña de ssh pasa a ser un paso opcional (sirve también con claves SSH).

## [0.3.0] - 2026-11-18
### Añadido
- Contraseñas en el llavero del sistema (KWallet / GNOME Keyring, vía Secret
  Service), escritas automáticamente cuando ssh pide `password:`.

## [0.2.0] - 2026-11-18
### Añadido
- Ventana de inicio con perfiles de servidor (nombre, servidor, usuario,
  puerto), guardados en `~/.config/retroterm/perfiles.toml`.
- Estilo retro (CSS) para las ventanas.

## [0.1.0] - 2026-11-18
### Añadido
- Primera versión: terminal VTE con fuente VT323, fósforo verde (y tema ámbar)
  y cursor de bloque parpadeante.
