#!/usr/bin/env bash
# Construit l'application et l'installe (ou la remplace) sur cette machine.
#
#   npm run installer            réinstalle la version courante
#   npm run installer -- patch   0.1.0 → 0.1.1, puis installe
#   npm run installer -- minor   0.1.0 → 0.2.0, puis installe
#
# Mac   : remplace /Applications/Spoor Vidéo.app.
# Linux : construit un paquet .deb et l'installe avec apt (demande le mot de passe sudo).
#
# La base, les vignettes et les journaux vivent dans le dossier de données de
# l'application (~/Library/Application Support/fr.victorprouff.spoorvideo sur Mac,
# ~/.local/share/fr.victorprouff.spoorvideo sous Linux) : remplacer l'application
# n'y touche pas.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="Spoor Vidéo"
OS="$(uname -s)"

case "$OS" in
  Darwin | Linux) ;;
  *)
    echo "✗ Système non pris en charge : $OS (Mac ou Linux seulement)" >&2
    exit 1
    ;;
esac

# Un terminal ouvert avant l'installation de Rust n'a pas cargo dans son PATH.
if ! command -v cargo >/dev/null && [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if [[ $# -gt 0 ]]; then
  # La version n'est écrite qu'ici : tauri.conf.json la lit dans package.json.
  npm version "$1" --no-git-tag-version >/dev/null
fi
VERSION=$(node -p "require('./package.json').version")
echo "→ Construction de $APP_NAME $VERSION ($OS)"

running() { pgrep -x "spoor-video" >/dev/null; }

wait_closed() {
  for _ in {1..20}; do running || return 0; sleep 0.25; done
}

if [[ "$OS" == "Darwin" ]]; then
  DEST="/Applications/$APP_NAME.app"
  BUILT="src-tauri/target/release/bundle/macos/$APP_NAME.app"

  # Seul le .app nous intéresse : pas de .dmg à fabriquer pour une installation locale.
  npm run tauri -- build --bundles app

  if running; then
    echo "→ Fermeture de l'application en cours"
    osascript -e "quit app \"$APP_NAME\"" || true
    wait_closed
  fi

  echo "→ Installation dans /Applications"
  rm -rf "$DEST"
  ditto "$BUILT" "$DEST"
else
  # Le paquet s'appelle spoor-video : dpkg refuse un nom accentué
  # (voir src-tauri/tauri.linux.conf.json).
  DEB_DIR="src-tauri/target/release/bundle/deb"
  DEB="$DEB_DIR/spoor-video_${VERSION}_$(dpkg --print-architecture).deb"

  # Les paquets des versions précédentes s'accumuleraient dans le dossier.
  rm -rf "$DEB_DIR"
  npm run tauri -- build --bundles deb

  if running; then
    echo "→ Fermeture de l'application en cours"
    # SIGTERM : l'application ne retire pas son verrou, mais le reprend sans
    # question au prochain lancement (verrou posé par cette même machine).
    pkill -x "spoor-video" || true
    wait_closed
  fi

  echo "→ Installation du paquet (sudo)"
  sudo apt install -y --reinstall "./$DEB"
fi

echo "✓ $APP_NAME $VERSION installée"
