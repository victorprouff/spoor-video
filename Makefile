# Commandes principales de Spoor Vidéo. `make` seul affiche l'aide.
#
#   make dev                 lance l'application en développement
#   make check               typecheck + tests Rust
#   make install             construit et installe la version courante (Mac ou Linux)
#   make version V=patch     passe à la version suivante et committe
#   make release V=patch     idem, puis installe
#
# V vaut patch (0.1.0 → 0.1.1), minor (0.1.0 → 0.2.0) ou major (0.1.0 → 1.0.0).
# M="…" ajoute un message au commit de version.

SHELL := /usr/bin/env bash
.SHELLFLAGS := -euo pipefail -c
.DEFAULT_GOAL := help

V ?= patch
M ?=

# Un terminal ouvert avant l'installation de Rust n'a pas cargo dans son PATH.
CARGO_ENV := [[ -f "$$HOME/.cargo/env" ]] && source "$$HOME/.cargo/env" || true

.PHONY: help dev free-port typecheck test check install version release

help: ## Affiche cette aide
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  make %-10s %s\n", $$1, $$2}'
	@echo
	@echo "  V=patch|minor|major (défaut : patch), M=\"message\" pour le commit de version"

dev: free-port ## Lance l'application en développement (Vite + fenêtre Tauri)
	@$(CARGO_ENV); npm run tauri:dev

free-port: ## Libère le port 1420 tenu par un ancien serveur Vite
	@pids=$$(lsof -ti:1420 || true); [[ -z "$$pids" ]] || { echo "→ Port 1420 libéré"; kill -9 $$pids; }

typecheck: ## Vérifie les types TypeScript
	@npm run typecheck

test: ## Lance les tests Rust (dont l'intégration avec ffmpeg)
	@$(CARGO_ENV); cd src-tauri && cargo test

check: typecheck test ## Typecheck puis tests Rust

install: ## Construit et installe la version courante (Mac : /Applications, Linux : .deb)
	@scripts/installer.sh

# Refuse de changer de version si rien n'a bougé depuis la précédente : ni commit depuis
# le dernier changement du numéro dans package.json, ni modification en cours. Les
# modifications en cours partent dans le commit de version, comme à l'habitude.
version: ## Passe à la version suivante (V=…) et committe, s'il y a eu des changements
	@case "$(V)" in patch|minor|major) ;; *) echo "✗ V doit valoir patch, minor ou major (reçu : $(V))" >&2; exit 1;; esac
	@last=$$(git log -G'"version"' -1 --format=%H -- package.json); \
	 current=$$(node -p "require('./package.json').version"); \
	 commits=$$([[ -n "$$last" ]] && git rev-list --count "$$last"..HEAD || echo 1); \
	 dirty=$$(git status --porcelain); \
	 if [[ "$$commits" == 0 && -z "$$dirty" ]]; then \
	   echo "✗ Rien n'a changé depuis la version $$current : pas de nouvelle version." >&2; exit 1; \
	 fi; \
	 echo "→ Depuis la version $$current : $$commits commit(s)$${dirty:+, et des modifications en cours :}"; \
	 [[ -z "$$dirty" ]] || git status --short
	@npm version "$(V)" --no-git-tag-version >/dev/null
	@new=$$(node -p "require('./package.json').version"); \
	 git add -A; \
	 msg="Version $$new"; [[ -z "$(M)" ]] || msg="$$msg : $(M)"; \
	 git commit -q -m "$$msg"; \
	 echo "✓ $$msg committée ($$(git rev-parse --short HEAD))"

release: version install ## Nouvelle version (V=…), commit, puis installation
