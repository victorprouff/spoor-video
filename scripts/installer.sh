#!/usr/bin/env bash
# Construit l'application et l'installe (ou la remplace) dans /Applications.
#
#   npm run installer            réinstalle la version courante
#   npm run installer -- patch   0.1.0 → 0.1.1, puis installe
#   npm run installer -- minor   0.1.0 → 0.2.0, puis installe
#
# La base, les vignettes et les journaux vivent dans le dossier de données de
# l'application (~/Library/Application Support/fr.victorprouff.spoorvideo) :
# remplacer l'application n'y touche pas.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="Spoor Vidéo"
DEST="/Applications/$APP_NAME.app"
BUILT="src-tauri/target/release/bundle/macos/$APP_NAME.app"

if [[ $# -gt 0 ]]; then
  # La version n'est écrite qu'ici : tauri.conf.json la lit dans package.json.
  npm version "$1" --no-git-tag-version >/dev/null
fi
VERSION=$(node -p "require('./package.json').version")
echo "→ Construction de $APP_NAME $VERSION"

# Seul le .app nous intéresse : pas de .dmg à fabriquer pour une installation locale.
npm run tauri -- build --bundles app

if pgrep -xq "spoor-video"; then
  echo "→ Fermeture de l'application en cours"
  osascript -e "quit app \"$APP_NAME\"" || true
  for _ in {1..20}; do pgrep -xq "spoor-video" || break; sleep 0.25; done
fi

echo "→ Installation dans /Applications"
rm -rf "$DEST"
ditto "$BUILT" "$DEST"

echo "✓ $APP_NAME $VERSION installée"
