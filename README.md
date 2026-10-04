# Spoor Vidéo

Dépouillement et analyse des vidéos de pièges photographiques.
Application de bureau **100 % locale** (Tauri + React/TS + SQLite), mono-poste.

La spécification fait foi : [`spec.md`](./spec.md).
Le contexte de l'application sœur : [`SPOOR-CONTEXTE.md`](./SPOOR-CONTEXTE.md).

## Développement

```bash
npm install
npm run tauri:dev
```

| Commande | Rôle |
|---|---|
| `npm run tauri:dev` | lance l'application (Vite + fenêtre Tauri) |
| `npm run tauri:build` | produit l'application installable |
| `npm run installer` | construit et installe l'application (`/Applications` sur Mac, paquet `.deb` sous Linux) |
| `npm run typecheck` | vérifie le TypeScript |
| `cargo test` (dans `src-tauri/`) | tests Rust, dont les migrations |

Le `Makefile` regroupe l'essentiel (`make` seul affiche l'aide) :

| Commande | Rôle |
|---|---|
| `make dev` | libère le port 1420 puis lance l'application en développement |
| `make check` | typecheck puis tests Rust |
| `make install` | construit et installe la version courante, Mac ou Linux |
| `make version V=patch` | passe à la version suivante et committe ; refuse si rien n'a changé depuis la précédente |
| `make release V=patch` | `make version` puis `make install` |

`V` vaut `patch` (défaut), `minor` ou `major` ; `M="…"` complète le message du commit
(`Version 0.13.2 : …`). Les modifications en cours partent dans le commit de version.

Prérequis : Node, Rust, les outils en ligne de commande Xcode, et **ffmpeg**
(`brew install ffmpeg`).

> ⚠️ ffmpeg n'est pas encore embarqué dans l'application : il est cherché d'abord dans
> les ressources de l'app, puis dans `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`
> et enfin le `PATH`. L'embarquer reste à faire avant toute distribution
> (`spec.md` §4 le prévoit).

## Installer le projet depuis zéro sur Linux (Pop!_OS)

Testé pour Pop!_OS 22.04 et 24.04 (base Ubuntu). Le code est le même que sur Mac :
seuls les prérequis et l'installation de l'application changent.

### 1. Paquets système

```bash
sudo apt update
sudo apt install -y build-essential curl wget file git pkg-config libssl-dev \
  libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev
```

Ce sont les dépendances de Tauri 2 : la fenêtre est une WebKitGTK.

### 2. Lecture des vidéos dans la fenêtre : GStreamer

Sous Linux, la balise `<video>` de WebKitGTK passe par **GStreamer**. Sans ses greffons,
l'application indexe bien les vidéos (c'est ffmpeg qui les lit) mais la lecture affiche
« Lecture impossible ». Le H.264 des pièges demande les greffons `bad`, `ugly` et
`libav` :

```bash
sudo apt install -y gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav
```

GStreamer ne sait pas lire le schéma `asset://` de Tauri : sous Linux, l'application sert
donc les vidéos par un petit serveur local (`127.0.0.1`, voir `spec.md` §8). Si la
lecture échoue malgré les greffons, chercher « serveur des vidéos » dans le journal.

### 3. ffmpeg

```bash
sudo apt install -y ffmpeg
```

Il s'installe dans `/usr/bin`, que l'application sonde déjà (voir `media.rs`). Vérifier
avec `ffprobe -version`.

### 4. Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accepter l'installation par défaut, puis ouvrir un nouveau terminal (ou
`source "$HOME/.cargo/env"`). Vérifier avec `rustc --version`.

### 5. Node

Le Node des dépôts Ubuntu est trop ancien pour Vite 8 (il faut au moins 20.19 ou
22.12). Passer par nvm :

```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/master/install.sh | bash
```

Ouvrir un nouveau terminal, puis :

```bash
nvm install --lts
```

Vérifier avec `node --version`.

### 6. Le projet

```bash
git clone https://github.com/victorprouff/spoor-video.git ~/code/spoor-video
cd ~/code/spoor-video
npm install
npm run tauri:dev
```

La première compilation Rust prend plusieurs minutes. Pour vérifier que tout est en
place :

```bash
npm run typecheck
cd src-tauri && cargo test
```

`cargo test` fabrique de vraies vidéos avec ffmpeg : s'il passe, ffmpeg est bien trouvé.

### 7. Installer l'application (facultatif)

Comme sur Mac, `npm run installer` (voir plus bas). Sous Linux, la commande construit
un paquet `.deb`, ferme l'application si elle est ouverte et installe le paquet avec
`apt` : elle demande le mot de passe `sudo`. Pour mettre à jour, la relancer.

Le paquet s'appelle `spoor-video` et non « Spoor Vidéo » : dpkg refuse un nom de paquet
avec un accent ou une majuscule. C'est le rôle de `src-tauri/tauri.linux.conf.json`,
que Tauri fusionne avec `tauri.conf.json` sous Linux seulement. Le nom affiché dans le
menu des applications reste « Spoor Vidéo » grâce à `src-tauri/linux/spoor-video.desktop`.

### Où vivent les données sous Linux

| | Mac | Linux |
|---|---|---|
| Base et vignettes | `~/Library/Application Support/fr.victorprouff.spoorvideo/` | `~/.local/share/fr.victorprouff.spoorvideo/` |
| Journal | `~/Library/Logs/fr.victorprouff.spoorvideo/` | `~/.local/share/fr.victorprouff.spoorvideo/logs/` |
| Corbeille | celle du Finder | celle du bureau (`~/.local/share/Trash`) |

En développement, le sous-dossier `dev/` s'applique aussi.

### Points d'attention

- **Partager la base avec le Mac via kDrive** : sur le Mac, Réglages → Base de données →
  « Déplacer la base… » vers un dossier kDrive (pas **dans** la racine des vidéos). Sous
  Linux, une fois kDrive synchronisé, « Ouvrir une autre base… » sur ce même dossier, puis
  Réglages → « Désigner le dossier sur cette machine… » pour indiquer où se trouve ici la
  racine des vidéos. Les chemins sont ensuite adaptés d'eux-mêmes à chaque changement de
  machine (voir `spec.md` §4, « Base partagée entre deux machines »).
  **Toujours fermer l'application avant de changer de machine** et laisser kDrive
  synchroniser : tant que la base est ouverte sur l'une, l'autre refuse de l'ouvrir.
  Mettre à jour les deux machines ensemble : une version plus ancienne refuse une base
  migrée par une plus récente.
- **kDrive** : installer le client kDrive pour Linux (AppImage d'Infomaniak) si l'on
  veut travailler sur les vraies vidéos, puis choisir sa racine dans Réglages.
- **Fenêtre blanche ou vide** au lancement, fréquente avec une carte NVIDIA : c'est un
  problème connu de WebKitGTK. Lancer avec
  `WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run tauri:dev`, et ajouter cette variable à
  `~/.profile` si cela règle le problème.
- **Port 1420 occupé** : `lsof -ti:1420 | xargs -r kill -9` (le `-r` de GNU évite
  d'appeler `kill` sans argument).

## Installer et mettre à jour l'application

L'application s'installe depuis les sources, sur Mac comme sous Linux. Aucun `.dmg`,
aucun téléchargement. La commande détecte le système.

```bash
npm run installer -- patch
```

La commande :

1. passe à la version suivante si on lui en donne une : `patch` (0.1.0 → 0.1.1),
   `minor` (0.1.0 → 0.2.0) ou `major` (0.1.0 → 1.0.0). Sans argument
   (`npm run installer`), elle réinstalle la version courante ;
2. construit l'application en mode release. Il faut compter environ 3 minutes la
   première fois, moins ensuite ;
3. ferme l'application si elle est ouverte ;
4. sur Mac, remplace `/Applications/Spoor Vidéo.app` ; sous Linux, installe le paquet
   `spoor-video` avec `apt` (mot de passe `sudo` demandé).

Pour mettre à jour après une séance de travail, il suffit de la relancer. **Les données
ne sont pas touchées** : elles vivent hors de l'application (voir plus bas).

La version n'est écrite qu'à un seul endroit, dans `package.json`, que `tauri.conf.json`
reprend. Le numéro de `Cargo.toml` n'est pas utilisé. La commande ne crée ni commit ni
tag git : on committe la nouvelle version soi-même.

L'application est construite sur la machine, donc Gatekeeper ne la bloque pas. Elle
trouve ffmpeg dans Homebrew. Sur un autre Mac, il faudrait d'abord embarquer ffmpeg
(voir l'avertissement plus haut).

## Où vivent les données

- **Base et vignettes** : par défaut dans
  `~/Library/Application Support/fr.victorprouff.spoorvideo/` (`spoor-video.sqlite` et
  `thumbnails/`). Elles se déplacent ensemble où l'on veut, par exemple à côté des vidéos,
  depuis **Réglages → Dossiers et indexation**. Déplacer copie et laisse l'original en
  place. Si la base est introuvable au démarrage (disque débranché), l'application le dit
  et ne crée jamais de base vide à la place (voir `spec.md` §4).
- **En développement** (`npm run tauri:dev`), tout vit à part, dans le sous-dossier
  `dev/` : on ne touche jamais aux données de l'application installée.
- **Journal** : `~/Library/Logs/fr.victorprouff.spoorvideo/spoor-video.log`.
- **Vidéos** : dans le dossier racine que tu configures, **indexées en place**.
  L'application ne les copie ni ne les déplace, et ne les supprime que sur demande
  explicite, vers la corbeille du système (voir `spec.md` §6).

## Migrations

Fichiers SQL numérotés dans `src-tauri/migrations/`, embarqués dans le binaire et
appliqués au démarrage, chacun dans sa transaction.

**On n'édite jamais une migration déjà livrée** — on en ajoute une.

## Réseau

L'application fonctionne hors ligne, **sauf les fonds de carte** (OpenStreetMap), chargés
à la demande quand une carte est affichée. Sans connexion, la carte reste grise et les
coordonnées se saisissent au clavier.

## Avancement

Étapes de `spec.md` §10 :

- [x] 1. Squelette Tauri + SQLite, migrations au boot, conventions CSS
- [x] 2. Indexation (parcours, empreinte, `ffprobe`, vignettes)
- [x] 3. Pièges et référentiel d'espèces
- [x] 4. Séquences : regroupement, scission, fusion
- [x] 5. Vue grille et annotation en masse
- [x] 5b. Suppressions (corbeille, trace conservée, `missing`)
- [x] 6. Mode plein écran au clavier
- [x] 7. Recherche et filtres
- [x] 8. Statistiques
- [ ] 9. Exports
