#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
#
# Construye los RPM de RetroTerm dentro de contenedores (podman o docker).
#
# Uso, desde la carpeta del proyecto:
#   ./empaquetado/construir-rpm.sh              # los tres
#   ./empaquetado/construir-rpm.sh opensuse     # solo uno: opensuse | fedora | el10
#
# Los RPM quedan en empaquetado/rpms/.
#
# ¿Por qué contenedores? Un ejecutable se enlaza con las librerías del
# sistema donde se compila (glibc, GTK, VTE). Compilando DENTRO de cada
# distribución, cada RPM funciona en la suya. No hace falta instalar nada
# de esas distribuciones en tu equipo: solo podman (o docker).
#
#   opensuse -> openSUSE Tumbleweed
#   fedora   -> la Fedora estable más reciente
#   el10     -> AlmaLinux 10 (vale también para Rocky Linux 10 y RHEL 10)
#
# EL9 (Rocky/Alma 9) NO está soportado: no tiene VTE para GTK 4.

set -euo pipefail

PROYECTO="$(cd "$(dirname "$0")/.." && pwd)"
EMPAQUETADO="$PROYECTO/empaquetado"
SALIDA="$EMPAQUETADO/rpms"
mkdir -p "$SALIDA"

# ---- Motor de contenedores
if command -v podman >/dev/null 2>&1; then
    MOTOR=podman
elif command -v docker >/dev/null 2>&1; then
    MOTOR=docker
else
    echo "Necesito podman (recomendado) o docker:  sudo zypper install podman" >&2
    exit 1
fi

# ---- Versión, leída de Cargo.toml (la primera línea 'version = "..."')
VERSION="$(grep -m1 '^version' "$PROYECTO/Cargo.toml" | sed -E 's/.*"(.*)".*/\1/')"
echo ">> RetroTerm $VERSION  (motor: $MOTOR)"

# ---- Fuente VT323 (SIL OFL 1.1): se descarga una vez y va dentro del RPM.
FUENTES_URL="https://github.com/google/fonts/raw/main/ofl/vt323"
if [[ ! -f "$EMPAQUETADO/VT323-Regular.ttf" ]]; then
    echo ">> Descargando la fuente VT323..."
    curl -fsSL -o "$EMPAQUETADO/VT323-Regular.ttf" "$FUENTES_URL/VT323-Regular.ttf"
    curl -fsSL -o "$EMPAQUETADO/OFL-VT323.txt" "$FUENTES_URL/OFL.txt"
fi

# ---- Qué hay que hacer en cada distribución: imagen, cómo instalar las
# herramientas de compilación, y la "etiqueta" que acaba en el nombre del RPM.
imagen() {
    case "$1" in
        opensuse) echo "registry.opensuse.org/opensuse/tumbleweed:latest" ;;
        fedora)   echo "registry.fedoraproject.org/fedora:latest" ;;
        el10)     echo "docker.io/library/almalinux:10" ;;
    esac
}

instalar_dependencias() {
    case "$1" in
        opensuse)
            echo "zypper -n refresh && zypper -n install --no-recommends \
                  gcc pkgconf curl rpm-build tar gzip pam-devel \
                  'pkgconfig(gtk4)' 'pkgconfig(vte-2.91-gtk4)' 'pkgconfig(dbus-1)'" ;;
        fedora)
            echo "dnf -y install gcc pkgconf-pkg-config curl rpm-build pam-devel \
                  gtk4-devel vte291-gtk4-devel dbus-devel" ;;
        el10)
            # Las cabeceras -devel de EL están en el repositorio CRB.
            echo "dnf -y install dnf-plugins-core && dnf config-manager --set-enabled crb && \
                  dnf -y install gcc pkgconf-pkg-config curl rpm-build pam-devel \
                  gtk4-devel vte291-gtk4-devel dbus-devel" ;;
    esac
}

etiqueta() {
    case "$1" in
        opensuse) echo ".opensuse" ;;
        fedora)   echo ".fedora" ;;
        el10)     echo ".el10" ;;
    esac
}

construir() {
    local destino="$1"
    echo
    echo "================ $destino ================"

    # Lo que se ejecuta DENTRO del contenedor. El proyecto está montado en /work.
    local guion="
        set -euo pipefail
        $(instalar_dependencias "$destino")

        # Rust con rustup (versión reciente, la misma en todas las distros).
        if [[ ! -x \$HOME/.cargo/bin/cargo ]]; then
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
                | sh -s -- -y --profile minimal
        fi
        source \$HOME/.cargo/env

        # Compilación. CARGO_TARGET_DIR aparte para no mezclar con tu target/.
        cd /work
        export CARGO_TARGET_DIR=/work/target-rpm-$destino
        cargo build --release

        # Fuentes del RPM: binario, .desktop, icono, fuente y licencia.
        RAIZ=/tmp/rpmbuild
        mkdir -p \$RAIZ/SOURCES
        cp \$CARGO_TARGET_DIR/release/retroterm \$RAIZ/SOURCES/
        cp /work/empaquetado/es.personal.RetroTerm.desktop \
           /work/empaquetado/es.personal.RetroTerm.svg \
           /work/empaquetado/VT323-Regular.ttf \
           /work/empaquetado/OFL-VT323.txt \
           /work/LICENSE /work/CHANGELOG.md \
           /work/empaquetado/pam/retroterm-suse \
           /work/empaquetado/pam/retroterm-redhat \$RAIZ/SOURCES/

        rpmbuild -bb /work/empaquetado/retroterm.spec \
            --define \"_topdir \$RAIZ\" \
            --define \"version_rt $VERSION\" \
            --define \"dist $(etiqueta "$destino")\"

        cp \$RAIZ/RPMS/*/retroterm-*.rpm /work/empaquetado/rpms/
    "

    # -v ...:Z    -> monta el proyecto (":Z" es para SELinux; donde no lo hay, se ignora)
    # volumen     -> guarda rustup y las descargas de crates entre ejecuciones
    "$MOTOR" run --rm \
        -v "$PROYECTO":/work:Z \
        -v "retroterm-rust-$destino":/root/.cargo \
        -v "retroterm-rustup-$destino":/root/.rustup \
        "$(imagen "$destino")" \
        bash -c "$guion"
}

# ---- Qué distribuciones construir
if [[ $# -eq 0 ]]; then
    DESTINOS=(opensuse fedora el10)
else
    DESTINOS=("$@")
fi

for destino in "${DESTINOS[@]}"; do
    case "$destino" in
        opensuse|fedora|el10) construir "$destino" ;;
        *) echo "Destino desconocido: $destino (usa opensuse, fedora o el10)" >&2; exit 1 ;;
    esac
done

echo
echo ">> Listo. RPM generados:"
ls -1 "$SALIDA"
