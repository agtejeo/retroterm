#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
#
# Construye los .deb de RetroTerm dentro de contenedores (podman o docker).
#
# Uso, desde la carpeta del proyecto:
#   ./empaquetado/construir-deb.sh               # los dos
#   ./empaquetado/construir-deb.sh ubuntu2404    # solo uno: debian13 | ubuntu2404
#
# Los .deb quedan en empaquetado/debs/.
#
#   debian13   -> Debian 13 "trixie"
#   ubuntu2404 -> Ubuntu 24.04 "noble" (y derivadas: Linux Mint 22, etc.)
#
# NO soportados (su VTE para GTK 4 es anterior a 0.72):
#   Debian 12 "bookworm" (0.70) y Ubuntu 22.04 "jammy" (0.68).
#
# Igual que con los RPM: se compila DENTRO de cada distribución para que el
# ejecutable enlace con sus librerías. El paquete lo genera `cargo deb`, que
# lee la sección [package.metadata.deb] de Cargo.toml.

set -euo pipefail

PROYECTO="$(cd "$(dirname "$0")/.." && pwd)"
EMPAQUETADO="$PROYECTO/empaquetado"
SALIDA="$EMPAQUETADO/debs"
mkdir -p "$SALIDA"

if command -v podman >/dev/null 2>&1; then
    MOTOR=podman
elif command -v docker >/dev/null 2>&1; then
    MOTOR=docker
else
    echo "Necesito podman (recomendado) o docker:  sudo zypper install podman" >&2
    exit 1
fi

VERSION="$(grep -m1 '^version' "$PROYECTO/Cargo.toml" | sed -E 's/.*"(.*)".*/\1/')"
echo ">> RetroTerm $VERSION  (motor: $MOTOR)"

# Fuente VT323 (SIL OFL 1.1), la misma que usan los RPM.
FUENTES_URL="https://github.com/google/fonts/raw/main/ofl/vt323"
if [[ ! -f "$EMPAQUETADO/VT323-Regular.ttf" ]]; then
    echo ">> Descargando la fuente VT323..."
    curl -fsSL -o "$EMPAQUETADO/VT323-Regular.ttf" "$FUENTES_URL/VT323-Regular.ttf"
    curl -fsSL -o "$EMPAQUETADO/OFL-VT323.txt" "$FUENTES_URL/OFL.txt"
fi

imagen() {
    case "$1" in
        debian13)   echo "docker.io/library/debian:trixie" ;;
        ubuntu2404) echo "docker.io/library/ubuntu:24.04" ;;
    esac
}

construir() {
    local destino="$1"
    echo
    echo "================ $destino ================"

    local guion="
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        # dpkg-dev trae dpkg-shlibdeps, que calcula las dependencias (\$auto).
        apt-get install -y --no-install-recommends \
            build-essential pkg-config curl ca-certificates dpkg-dev \
            libgtk-4-dev libvte-2.91-gtk4-dev libdbus-1-dev libpam0g-dev

        if [[ ! -x \$HOME/.cargo/bin/cargo ]]; then
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
                | sh -s -- -y --profile minimal
        fi
        source \$HOME/.cargo/env

        # cargo-deb se compila una vez y queda en el volumen de ~/.cargo.
        if ! command -v cargo-deb >/dev/null 2>&1; then
            cargo install cargo-deb --locked
        fi

        cd /work
        export CARGO_TARGET_DIR=/work/target-deb-$destino
        # Compila en modo release y genera el .deb. La versión lleva la
        # distribución, p. ej. 0.12.0-1+ubuntu2404, para distinguir los paquetes.
        cargo deb --deb-version '$VERSION-1+$destino' --output /work/empaquetado/debs/
    "

    "$MOTOR" run --rm \
        -v "$PROYECTO":/work:Z \
        -v "retroterm-rust-deb-$destino":/root/.cargo \
        -v "retroterm-rustup-deb-$destino":/root/.rustup \
        "$(imagen "$destino")" \
        bash -c "$guion"
}

if [[ $# -eq 0 ]]; then
    DESTINOS=(debian13 ubuntu2404)
else
    DESTINOS=("$@")
fi

for destino in "${DESTINOS[@]}"; do
    case "$destino" in
        debian13|ubuntu2404) construir "$destino" ;;
        *) echo "Destino desconocido: $destino (usa debian13 o ubuntu2404)" >&2; exit 1 ;;
    esac
done

echo
echo ">> Listo. Paquetes generados:"
ls -1 "$SALIDA"
