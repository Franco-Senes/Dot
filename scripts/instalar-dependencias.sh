#!/usr/bin/env bash
# Instala las dependencias para compilar Dot para Linux.
# Compatible con Debian/Ubuntu, Fedora, Arch y openSUSE.
set -euo pipefail

say() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

if [ -r /etc/os-release ]; then . /etc/os-release; else ID=unknown; fi
FAMILY="${ID} ${ID_LIKE:-}"

say "Distribución detectada: ${PRETTY_NAME:-$ID}"

case "$FAMILY" in
  *debian*|*ubuntu*)
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config git curl meson ninja-build \
      libgtk-4-dev libadwaita-1-dev libwebkitgtk-6.0-dev libsecret-1-dev \
      libsqlite3-dev libssl-dev gettext desktop-file-utils appstream \
      flatpak flatpak-builder xvfb weston socat python3
    ;;
  *fedora*|*rhel*)
    sudo dnf install -y gcc gcc-c++ pkgconf-pkg-config git curl meson ninja-build \
      gtk4-devel libadwaita-devel webkitgtk6.0-devel libsecret-devel \
      sqlite-devel openssl-devel gettext desktop-file-utils appstream \
      flatpak flatpak-builder xorg-x11-server-Xvfb weston socat python3
    ;;
  *arch*)
    sudo pacman -S --needed --noconfirm base-devel pkgconf git curl meson ninja \
      gtk4 libadwaita webkitgtk-6.0 libsecret sqlite openssl gettext \
      desktop-file-utils appstream flatpak flatpak-builder xorg-server-xvfb \
      weston socat python
    ;;
  *suse*)
    sudo zypper install -y gcc gcc-c++ pkg-config git curl meson ninja \
      gtk4-devel libadwaita-devel webkit2gtk4-devel libsecret-devel \
      sqlite3-devel libopenssl-devel gettext-tools desktop-file-utils \
      AppStream flatpak flatpak-builder xvfb-run weston socat python3
    echo "Aviso: en openSUSE el paquete de WebKitGTK 6.0 puede llamarse distinto."
    echo "Comprueba con: zypper search webkit | grep -i 6"
    ;;
  *)
    echo "Distribución no reconocida. Instala a mano: Rust, meson, ninja,"
    echo "GTK4, libadwaita, WebKitGTK 6.0 (webkitgtk-6.0), libsecret, sqlite,"
    echo "flatpak, flatpak-builder, xvfb, weston y socat."
    exit 1
    ;;
esac

say "Rust (rustup)"
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi
rustup update stable
rustup component add rustfmt clippy

say "Flathub y SDK de GNOME"
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
echo "El SDK de GNOME se instala solo la primera vez que ejecutes flatpak-builder"
echo "(o el agente lo instalará en la Fase 8 con la versión correcta)."

say "Comprobación"
ok=1
for lib in gtk4 libadwaita-1 webkitgtk-6.0 libsecret-1; do
  if pkg-config --exists "$lib"; then
    printf '  ✔ %-15s %s\n' "$lib" "$(pkg-config --modversion "$lib")"
  else
    printf '  ✘ %-15s NO ENCONTRADO\n' "$lib"; ok=0
  fi
done
printf '  ✔ %-15s %s\n' "rustc" "$(rustc --version)"

if [ "$ok" = 1 ]; then
  say "Todo listo. Siguiente paso: bash scripts/clonar-upstream.sh"
else
  say "Faltan bibliotecas. Revisa los mensajes de arriba."
  exit 1
fi
