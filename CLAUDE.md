# Spoor Vidéo — mémoire du projet

Application de bureau **100 % locale** pour dépouiller et analyser les vidéos de pièges
photographiques. Tauri (Rust) + React/TS + SQLite, mono-poste, mono-utilisateur.

**La spécification fait foi : [`spec.md`](./spec.md).** Elle porte les décisions *et
leurs raisons*. Avant de changer un comportement, y lire pourquoi il est ainsi ; après
l'avoir changé, l'y répercuter. [`TODO.md`](./TODO.md) appartient à Victor : le lire,
ne pas y écrire sans le lui demander.

Application sœur : `~/code/spoor` (carnet de terrain web). Aucun code partagé.
Le contexte repris d'elle est dans [`SPOOR-CONTEXTE.md`](./SPOOR-CONTEXTE.md).

## Commandes

```bash
npm run tauri:dev      # lance l'application (Vite + fenêtre Tauri)
npm run typecheck      # tsc --noEmit
cd src-tauri && cargo test
npm run installer      # construit et remplace /Applications/Spoor Vidéo.app
npm run installer -- patch   # idem, en passant d'abord à la version suivante
```

La version n'est écrite que dans `package.json` (`tauri.conf.json` y renvoie).

Prérequis : Node, Rust, outils Xcode, **ffmpeg** (`brew install ffmpeg`).
Sous Linux (Pop!_OS) : voir la section dédiée du README (WebKitGTK, GStreamer,
`npm run installer` ne marche pas).

## Organisation

| | |
|---|---|
| `src-tauri/src/` | un module par domaine : `scan`, `sequences`, `annotations`, `grid`, `videos`, `stats`, `export`, `deletions`, `positions`, `pending`, `sun`, `traps`, `species`, `media`, `hash`, `location`, `lock`, `root`, `stream` (vidéos servies en HTTP local, Linux) |
| `src-tauri/migrations/` | SQL numéroté, embarqué dans le binaire, appliqué au démarrage |
| `src/views/` | un fichier par écran ; `src/components/` pour le partagé |
| Tests | **Rust uniquement** — 163, dont 8 d'intégration sur de vraies vidéos générées par ffmpeg. Pas de test front, pas d'ESLint. |

Deux onglets permanents, Statistiques (défaut) et Vidéos ; **Dépouiller** n'apparaît que
s'il reste du travail (import ou séquences non dépouillées) ; le reste (dossiers, base,
pièges, espèces, thème) est dans **Réglages** (`src/views/Settings.tsx`). Voir `spec.md` §8.

## Règles que le code respecte — ne pas les casser par mégarde

1. **Les dates sont de l'heure murale au piège.** Jamais converties dans le fuseau de
   la machine : `src/format.ts` découpe la chaîne sans passer par `new Date()`. Une
   conversion réintroduirait un décalage d'une à deux heures selon la saison.
2. **On n'édite jamais une migration livrée**, on en ajoute une.
3. **Une séquence annotée est gelée** : le regroupement automatique ne la défait pas.
   Sans cela, relancer une passe effacerait du travail.
4. **Rien ne disparaît en silence.** Vidéo sans date, sans position, hors dossier
   rattaché, écartée, illisible : chacune est comptée et nommée dans l'interface. Deux
   bugs de cette famille ont déjà été corrigés (graphique tronqué, pastille qui mentait).
5. **La pastille « en attente » doit dire ce que la passe fera vraiment**, sinon le
   message ne peut jamais partir.
6. **Position : le piège donne le défaut, la vidéo garde la sienne.** Déplacer un piège
   n'a aucun effet rétroactif.
7. **Suppressions** : corbeille du système, jamais d'effacement ; rien hors du dossier
   racine ; un échec ne marque pas la vidéo supprimée.
8. **La base introuvable n'est jamais remplacée par une vide.** Elle peut vivre hors
   du dossier de données (`location.rs`) ; en `tauri dev`, elle vit dans `dev/`.
   Elle peut être **partagée entre le Mac et Linux via kDrive** : verrou (`lock.rs`),
   racine propre à chaque machine et chemins réécrits à l'ouverture (`root.rs`), journal
   `delete` et non WAL, refus d'une base plus récente que l'application. Voir `spec.md` §4.
9. **Français partout** : interface, commentaires, noms de tests. Le code technique
   (types, champs SQL) reste en anglais.

## Vérifier son travail

La fenêtre Tauri n'est pas pilotable depuis Claude Code : impossible de cliquer dedans.
Deux moyens, à utiliser tous les deux :

- **Rust** : `cargo test`. Les tests d'intégration fabriquent de vraies vidéos avec
  ffmpeg — c'est là que se trouvent les régressions de fond.
- **Interface** : créer une **page d'aperçu jetable** à la racine (`x-preview.html` +
  `src/__preview.tsx`), la regarder via le navigateur intégré sur
  `http://localhost:1420/x-preview.html` avec des données réalistes, en thème clair
  **et** sombre, puis **la supprimer avant de committer**. Plusieurs défauts n'ont été
  vus que comme ça.

Le serveur Vite tient le port 1420 : le libérer (`lsof -ti:1420 | xargs kill -9`) avant
de relancer, sinon `tauri dev` échoue.

## Reste ouvert

- **ffmpeg n'est pas embarqué.** Il est cherché dans les ressources de l'app, puis
  `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, puis le `PATH`. À empaqueter avant
  toute distribution, sinon l'application installée ne lit rien.
- **Lecture des `.avi` non vérifiée** dans la fenêtre. Si les pièges en produisent, il
  faudra décider d'un transcodage à l'indexation.
- La racine des vidéos de Victor est dans **kDrive** : une suppression peut être défaite
  par la synchronisation, et un fichier écarté peut réapparaître.
