# BLOQUEOS.md — Registro de bloqueos de entorno y limitaciones

## 1. Dependencias nativas del sistema anfitrión (Solus 4.9 Serenity) — RESUELTO CON SDK LOCAL

- **Problema detectado:** `scripts/instalar-dependencias.sh` solo contempla Debian/Ubuntu, Fedora/RHEL, Arch y openSUSE. En este equipo (**Solus 4.9 Serenity**, gestor `eopkg`), el script terminó con `Distribución no reconocida`, y `sudo -n true` requiere contraseña interactiva (`sudo: a password is required`). En consecuencia, en el sistema anfitrión faltaban `git`, `gcc`, `pkg-config`, `rustc`, `cargo`, `flatpak-builder` y los paquetes `-devel` de GTK4, libadwaita, WebKitGTK 6.0 y libsecret.
- **Solución autónoma sin tocar nada fuera del proyecto:** Se instaló un entorno de compilación completo y 100 % aislado dentro de `.toolchain/` (ignorado en `.gitignore`, usando `FLATPAK_USER_DIR` y `HOME` locales al proyecto) basado en Flathub:
  - `org.gnome.Sdk//51` (GTK `4.24.1`, libadwaita `1.10.0`, WebKitGTK 6.0 `2.54.1`, libsecret `0.21.8.2`, SQLite `3.53.4`, OpenSSL `3.5.9`, Git `2.56.0`, AppStream `1.1.6`).
  - `org.freedesktop.Sdk.Extension.rust-stable//26.08` (`rustc 1.98.0`, `cargo 1.98.0`, `clippy 0.1.98`, `rustfmt 1.9.0`).
  - `org.flatpak.Builder//stable` para construir el paquete Flatpak local.
- **Impacto:** Ninguno para el desarrollo, compilación, tests unitarios, tests headless (usando `/usr/bin/xvfb-run` del sistema anfitrión conectado al SDK) ni empaquetado Flatpak.
- **Comando para el usuario si desea compilar directamente en el host Solus (fuera de Flatpak):**
  ```bash
  sudo eopkg update-repo
  sudo eopkg install -c system.devel
  sudo eopkg install git rust cargo libgtk-4-devel libadwaita-devel webkitgtk-6.0-devel \
    libsecret-devel sqlite3-devel openssl-devel flatpak-builder xorg-server-xvfb weston socat
  ```
