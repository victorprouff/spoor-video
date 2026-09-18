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
| `npm run typecheck` | vérifie le TypeScript |
| `cargo test` (dans `src-tauri/`) | tests Rust, dont les migrations |

Prérequis : Node, Rust, et les outils en ligne de commande Xcode.

## Où vivent les données

- **Base** : `~/Library/Application Support/fr.victorprouff.spoorvideo/spoor-video.sqlite`
  — jamais à côté des vidéos, pour rester lisible disque débranché.
- **Vidéos** : dans le dossier racine que tu configures, **indexées en place**.
  L'application ne les copie ni ne les déplace, et ne les supprime que sur demande
  explicite, vers la corbeille du système (voir `spec.md` §6).

## Migrations

Fichiers SQL numérotés dans `src-tauri/migrations/`, embarqués dans le binaire et
appliqués au démarrage, chacun dans sa transaction.

**On n'édite jamais une migration déjà livrée** — on en ajoute une.

## Avancement

Étapes de `spec.md` §10 :

- [x] 1. Squelette Tauri + SQLite, migrations au boot, conventions CSS
- [ ] 2. Indexation (parcours, empreinte, `ffprobe`, vignettes)
- [ ] 3. Pièges et référentiel d'espèces
- [ ] 4. Séquences : regroupement, scission, fusion
- [ ] 5. Vue grille et annotation en masse
- [ ] 5b. Suppressions (corbeille, trace conservée, `missing`)
- [ ] 6. Mode plein écran au clavier
- [ ] 7. Recherche et filtres
- [ ] 8. Statistiques
- [ ] 9. Exports
