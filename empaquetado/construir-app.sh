#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
#
# Construye RetroTerm.app para macOS y la instala en ~/Applications.
#
# Uso, desde la carpeta del proyecto (solo en macOS):
#   ./empaquetado/construir-app.sh                 # instala en ~/Applications
#   ./empaquetado/construir-app.sh /Applications   # u otra carpeta
#
# Para actualizar la app tras cambiar el código, se vuelve a ejecutar.
#
# Una aplicación de macOS es solo una CARPETA con una estructura fija:
#
#   RetroTerm.app/Contents/
#   ├─ Info.plist          nombre, identificador, versión, icono
#   ├─ MacOS/lanzador      script zsh: carga tu entorno y arranca...
#   ├─ MacOS/retroterm     ...el ejecutable de `cargo build --release`
#
# (El lanzador NO puede llamarse «RetroTerm»: el disco de macOS no distingue
# mayúsculas de minúsculas, y pisaría al ejecutable «retroterm».)
#   └─ Resources/RetroTerm.icns   el icono, a partir del SVG del proyecto
#
# OJO: la app usa GTK 4 y VTE de Homebrew (/opt/homebrew o /usr/local), no
# las lleva dentro. Sirve para ESTE Mac, o para otro que también las tenga
# (brew install vte3 adwaita-icon-theme).

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Este script es para macOS. En Linux: construir-rpm.sh o construir-deb.sh" >&2
    exit 1
fi

PROYECTO="$(cd "$(dirname "$0")/.." && pwd)"
EMPAQUETADO="$PROYECTO/empaquetado"
DESTINO="${1:-$HOME/Applications}"
VERSION="$(grep -m1 '^version' "$PROYECTO/Cargo.toml" | sed -E 's/.*"(.*)".*/\1/')"
echo ">> RetroTerm $VERSION para macOS ($(uname -m))"

for orden in cargo rsvg-convert iconutil; do
    if ! command -v "$orden" >/dev/null 2>&1; then
        echo "Falta «$orden». Mira «Compilar desde el código» en el README" \
             "(rsvg-convert viene con: brew install librsvg)." >&2
        exit 1
    fi
done

# ---- 1. Compilar optimizado (el binario queda en target/release/retroterm)
echo ">> Compilando (cargo build --release)..."
(cd "$PROYECTO" && cargo build --release)

# ---- 2. Montar la app en una carpeta TEMPORAL
#
# No dentro del proyecto (p. ej. target/app): Spotlight indexa esa carpeta y
# Launchpad mostraría DOS RetroTerm, la instalada y la copia de trabajo.
# `mktemp -d` crea una carpeta vacía con nombre único en /tmp; `trap ... EXIT`
# la borra al terminar el script, tanto si acaba bien como si falla.
TRABAJO="$(mktemp -d)"
trap 'rm -rf "$TRABAJO"' EXIT
APP_TMP="$TRABAJO/RetroTerm.app"
# Restos de la 0.29.0, que montaba la app en target/app: se quitan, y se
# le dice a macOS que olvide esa copia («lsregister -u»).
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
if [[ -d "$PROYECTO/target/app" ]]; then
    [[ -x "$LSREGISTER" ]] && "$LSREGISTER" -u "$PROYECTO/target/app/RetroTerm.app" 2>/dev/null || true
    rm -rf "$PROYECTO/target/app"
fi
mkdir -p "$APP_TMP/Contents/MacOS" "$APP_TMP/Contents/Resources"
cp "$PROYECTO/target/release/retroterm" "$APP_TMP/Contents/MacOS/retroterm"

# El lanzador. Al abrir una app desde el Finder o el Dock, macOS NO lee tu
# ~/.zprofile: no habría PATH de Homebrew, y las pestañas empezarían en «/».
# Por eso arrancamos RetroTerm a través de tu shell como shell de inicio de
# sesión (-l), igual que al abrir una terminal, y desde tu carpeta personal.
# (Las comillas en 'EOF' evitan que bash sustituya nada al escribirlo.)
cat > "$APP_TMP/Contents/MacOS/lanzador" <<'EOF'
#!/bin/zsh
# Lanzador de RetroTerm.app (lo genera empaquetado/construir-app.sh).
# ${0:A:h} = carpeta de este script, con la ruta completa (zsh).
binario="${0:A:h}/retroterm"
# La shell de tu usuario, según el sistema (SHELL puede faltar desde el Finder).
shell_usuario="$(dscl . -read "$HOME" UserShell 2>/dev/null | awk '{print $2}')"
export SHELL="${shell_usuario:-/bin/zsh}"
cd "$HOME"
# `exec`: la shell se convierte en RetroTerm (mismo proceso), sin quedarse
# esperando detrás. Dentro de -c, "$0" es el primer argumento tras la orden.
exec "$SHELL" -l -c 'exec "$0"' "$binario"
EOF
chmod +x "$APP_TMP/Contents/MacOS/lanzador"

cat > "$APP_TMP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                 <string>RetroTerm</string>
    <key>CFBundleDisplayName</key>          <string>RetroTerm</string>
    <key>CFBundleIdentifier</key>           <string>es.personal.RetroTerm</string>
    <key>CFBundleExecutable</key>           <string>lanzador</string>
    <key>CFBundleIconFile</key>             <string>RetroTerm</string>
    <key>CFBundlePackageType</key>          <string>APPL</string>
    <key>CFBundleShortVersionString</key>   <string>$VERSION</string>
    <key>CFBundleVersion</key>              <string>$VERSION</string>
    <key>CFBundleDevelopmentRegion</key>    <string>es</string>
    <key>LSMinimumSystemVersion</key>       <string>11.0</string>
    <key>LSApplicationCategoryType</key>    <string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key>      <true/>
    <key>NSHumanReadableCopyright</key>     <string>© 2026 Alfonso García Tejeo · GPL-3.0-or-later</string>
</dict>
</plist>
EOF

# ---- 3. Icono: del SVG a un .icns (todos los tamaños que pide macOS)
echo ">> Generando el icono..."
ICONSET="$TRABAJO/RetroTerm.iconset"
mkdir -p "$ICONSET"
for tamano in 16 32 128 256 512; do
    doble=$((tamano * 2))
    rsvg-convert -w "$tamano" -h "$tamano" "$EMPAQUETADO/es.personal.RetroTerm.svg" \
        -o "$ICONSET/icon_${tamano}x${tamano}.png"
    rsvg-convert -w "$doble" -h "$doble" "$EMPAQUETADO/es.personal.RetroTerm.svg" \
        -o "$ICONSET/icon_${tamano}x${tamano}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$APP_TMP/Contents/Resources/RetroTerm.icns"

# Firma "ad hoc" (sin certificado): en Apple Silicon todo ejecutable tiene que
# ir firmado, y así la app queda coherente. No es una firma de Apple.
codesign --force --deep --sign - "$APP_TMP" >/dev/null 2>&1 \
    || echo "   (aviso: no se pudo firmar ad hoc; la app funciona igual en este Mac)"

# ---- 4. Instalar (sustituye la versión anterior, si la hay)
mkdir -p "$DESTINO"
rm -rf "$DESTINO/RetroTerm.app"
cp -R "$APP_TMP" "$DESTINO/"
# Avisar a macOS de que la app (y su icono) han cambiado.
[[ -x "$LSREGISTER" ]] && "$LSREGISTER" -f "$DESTINO/RetroTerm.app" || true

# ---- 5. Fuente VT323 (SIL OFL 1.1), si no está ya instalada
FUENTES_URL="https://github.com/google/fonts/raw/main/ofl/vt323"
if ! ls "$HOME/Library/Fonts" /Library/Fonts 2>/dev/null | grep -qi 'vt323'; then
    if [[ ! -f "$EMPAQUETADO/VT323-Regular.ttf" ]]; then
        echo ">> Descargando la fuente VT323..."
        curl -fsSL -o "$EMPAQUETADO/VT323-Regular.ttf" "$FUENTES_URL/VT323-Regular.ttf"
        curl -fsSL -o "$EMPAQUETADO/OFL-VT323.txt" "$FUENTES_URL/OFL.txt"
    fi
    mkdir -p "$HOME/Library/Fonts"
    cp "$EMPAQUETADO/VT323-Regular.ttf" "$HOME/Library/Fonts/"
    echo ">> Fuente VT323 instalada en ~/Library/Fonts"
fi

echo ">> Listo: $DESTINO/RetroTerm.app"
echo "   Ábrela desde Launchpad o Spotlight; para el Dock: clic derecho en su"
echo "   icono mientras está abierta → Opciones → Mantener en el Dock."
