#!/usr/bin/env bash
# Clona el proyecto original en ./upstream (solo para leerlo; no se sube a git).
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d upstream/.git ]; then
  echo "upstream/ ya existe; actualizando..."
  git -C upstream pull --ff-only
else
  git clone --depth 1 https://github.com/driceroland/Search upstream
fi

touch .gitignore
for line in "upstream/" "target/" "_build/" ".flatpak-builder/" "build-dir/" "repo/"; do
  grep -qxF "$line" .gitignore || echo "$line" >> .gitignore
done

echo
echo "Listo. Archivos Swift en el original:"
find upstream/Sources -name '*.swift' | wc -l
echo
echo "Siguiente paso: git init && git add -A && git commit -m 'Kit inicial del port'"
echo "Luego abre Antigravity aquí y pega prompts/00-analisis.md"
