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
| `npm run installer` | construit et installe l'application dans `/Applications` |
| `npm run typecheck` | vérifie le TypeScript |
| `cargo test` (dans `src-tauri/`) | tests Rust, dont les migrations |

Prérequis : Node, Rust, les outils en ligne de commande Xcode, et **ffmpeg**
(`brew install ffmpeg`).

> ⚠️ ffmpeg n'est pas encore embarqué dans l'application : il est cherché d'abord dans
> les ressources de l'app, puis dans `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`
> et enfin le `PATH`. L'embarquer reste à faire avant toute distribution
> (`spec.md` §4 le prévoit).

## Installer et mettre à jour l'application

L'application s'installe sur le Mac depuis les sources. Aucun `.dmg`, aucun
téléchargement.

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
4. remplace `/Applications/Spoor Vidéo.app`.

Pour mettre à jour après une séance de travail, il suffit de la relancer. **Les données
ne sont pas touchées** : elles vivent hors de l'application (voir plus bas).

La version n'est écrite qu'à un seul endroit, dans `package.json`, que `tauri.conf.json`
reprend. Le numéro de `Cargo.toml` n'est pas utilisé. La commande ne crée ni commit ni
tag git : on committe la nouvelle version soi-même.

L'application est construite sur la machine, donc Gatekeeper ne la bloque pas. Elle
trouve ffmpeg dans Homebrew. Sur un autre Mac, il faudrait d'abord embarquer ffmpeg
(voir l'avertissement plus haut).

> ⚠️ L'application installée et `npm run tauri:dev` partagent **la même base**. Une
> séance de développement travaille donc sur les vraies données, et une nouvelle
> migration s'y applique dès le premier lancement en développement.

## Où vivent les données

- **Base** : `~/Library/Application Support/fr.victorprouff.spoorvideo/spoor-video.sqlite`
  — jamais à côté des vidéos, pour rester lisible disque débranché.
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
