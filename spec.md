# Spoor Vidéo — spécification V1

> Application de dépouillement et d'analyse des vidéos de pièges photographiques.
> Application sœur de [Spoor](./SPOOR-CONTEXTE.md), sans code partagé, mais avec un
> modèle de données délibérément aligné.

---

## 1. Intention

Je pose des pièges photographiques en forêt. Ils se déclenchent au passage d'un animal
et produisent des vidéos que je récupère sur une carte SD, puis range sur un drive local.

L'application doit me permettre de :

1. **indexer** ces vidéos sans les déplacer, en récupérant automatiquement leur date et heure ;
2. **dépouiller** rapidement un lot : dire quelle espèce, avec quelle confiance, ou « rien » ;
3. **analyser** : rythmes d'activité, saisonnalité, comparaison entre emplacements.

Philosophie reprise de Spoor, et non négociable : **léger et simple, pas de sur-ingénierie.**

---

## 2. Décisions arrêtées

| Sujet | Décision |
|---|---|
| Exécution | **100 % local**, mono-poste, mono-utilisateur. Aucun serveur, aucun compte. |
| Réseau | **Une seule exception** : les fonds de carte OpenStreetMap. Sans connexion, la carte reste grise et les coordonnées restent saisissables au clavier. Tout le reste fonctionne hors ligne. |
| Runtime | **Tauri** (cœur Rust, front React + TypeScript + Vite) |
| Base | **SQLite**, un fichier unique dans le dossier de données de l'application |
| Rangement | **Un dossier racine**, contenant **un dossier par caméra**. L'arborescence porte le rattachement au piège. |
| Fichiers vidéo | **Indexés en place**, jamais copiés ni déplacés. **Supprimés uniquement sur demande explicite** (§6). |
| À dépouiller | Toute vidéo présente sous la racine et **absente de la base** est en attente de catégorisation. |
| Date / heure | **Métadonnées du conteneur** (`ffprobe`), avec correction manuelle possible |
| Espèces | **Liste courte locale**, pré-remplie, extensible librement |
| Identification | **Manuelle en V1.** Modèle prêt pour une détection automatique ultérieure. |
| Unité d'annotation | **Vidéo et séquence**, les deux niveaux stockés |
| Pont avec Spoor | **Aucun en V1**, mais nomenclature et niveaux de confiance identiques |
| Effort de piégeage | **Hors périmètre V1** (voir §9) |
| CSS | CSS pur, variables dans `:root`, thème clair/sombre — conventions Spoor |

---

## 3. Modèle de données

Quatre tables portantes. Les conventions Spoor qui ont fait leurs preuves sont reprises :
**UUID généré côté application**, `created_at` / `updated_at`, **soft delete** (`deleted_at`),
et surtout **la séparation entre donnée extraite automatiquement et correction manuelle**.

### `traps` — le piège, lieu durable

L'emplacement d'un piège est **durable** ; chaque vidéo est un **événement daté** rattaché à lui.
C'est la leçon la plus importante de Spoor (`sites` / passages), appliquée ici dès le départ.

- `id`, `name` (ex. « Coulée du ruisseau »), `camera_name` (tel qu'écrit dans le bandeau incrusté)
- `latitude`, `longitude`, `altitude_m` — facultatifs
- `notes`, `active`
- `created_at`, `updated_at`, `deleted_at`

**La position du piège n'est qu'une valeur par défaut.** Elle est copiée sur chaque vidéo
au moment de l'indexation ; ensuite la vidéo garde la sienne. Déplacer un piège n'a donc
**aucun effet rétroactif** : les captures d'il y a deux ans restent là où elles ont été
faites. C'est le même principe que `recorded_at` / `recorded_at_manual` — la donnée et la
correction cohabitent, rien n'est réécrit dans le dos.

⚠️ Conséquence à connaître : un piège déplacé garde son identité, donc la **comparaison
entre emplacements** (§7) regroupe sous un même nom des positions différentes. Quand le
déplacement est important, créer un nouveau piège reste plus juste.

### `videos` — le fichier, un par ligne

- `id`
- `file_path` (absolu), `file_name`, `file_size`, `content_hash`
- `trap_id`
- `recorded_at` — **date de la capture**, extraite de `ffprobe`
- `recorded_at_manual` — correction éventuelle ; **la valeur d'origine n'est jamais écrasée**
- `duration_s`, `width`, `height`, `fps`
- `latitude`, `longitude`, `altitude_m` — **position propre à la capture**, copiée du
  piège à l'indexation puis ajustable ; `position_manual` empêche toute réécriture
- `thumbnail_path` — vignette extraite par ffmpeg, dans les données de l'application
- `sequence_id`
- `file_state` — `present` | `purged` | `missing` (voir §6)
- `file_removed_at`, `file_removed_reason`
- `imported_at`, `reviewed_at`
- `created_at`, `updated_at`, `deleted_at`

`content_hash` (empreinte des premiers et derniers Mo + taille) sert à **deux** choses :
ne jamais réimporter deux fois le même fichier, et **retrouver une vidéo déplacée** dans
l'arborescence plutôt que de la déclarer perdue.

### `sequences` — le passage

Un même animal déclenche souvent plusieurs vidéos d'affilée. Une séquence regroupe les
vidéos d'**un même piège** séparées de moins de `N` minutes (défaut : 10, réglable).

- `id`, `trap_id`
- `started_at`, `ended_at`, `video_count`
- `auto_grouped` — vrai tant que je n'ai pas scindé ou fusionné à la main
- `notes`
- `created_at`, `updated_at`, `deleted_at`

**Une séquence longue reste une séquence longue.** Si une espèce déclenche la caméra
pendant deux heures, c'est *un* passage de deux heures, avec sa durée et son nombre de
déclenchements — et non trente passages. Les deux chiffres sont conservés et exploités
séparément dans les statistiques (§6).

Les annotations portent sur la **séquence**. Une vidéo hérite de l'annotation de sa
séquence ; on peut annoter une vidéo isolément quand une seule du lot montre autre chose.

### `species` et `sequence_species`

- `species` : `id`, `common_name` (français, fait foi), `scientific_name`, `group`
  (mammifère / oiseau / autre), `color`, `sort_order`, `shortcut_key`, `builtin`
- `sequence_species` : `sequence_id`, `species_id`, `confidence`, `count_min`, `count_max`, `notes`

Trois règles reprises de Spoor, chacune payée une fois là-bas :

1. **Plusieurs espèces par séquence**, chacune avec **sa propre** confiance.
2. **Confiance à trois niveaux** : `certain` / `probable` / `possible`. Pas de note sur 10 —
   une échelle fine donne une fausse précision et personne ne sait trancher entre 6 et 7.
3. **Espèce ≠ tag.** Les tags libres (`tags`, `sequence_tags`) existent pour le reste
   (« juvénile », « comportement de marquage », « bandeau illisible ») et filtrent en **ET**,
   là où les espèces filtrent en **OU**. Pas de catégories de tags : détour inutile dans Spoor.

Un état explicite complète l'espèce : `empty` (fausse déclenche, rien de visible),
`unidentified` (quelque chose passe, non identifiable), `human`, `vehicle`, `livestock`.
Ces cas sont nombreux et ne doivent pas polluer le référentiel d'espèces.

### `purged_videos` — les suppressions définitives

`content_hash`, `file_name`, `purged_at`. Uniquement le cas (a) du §6 : ce qui a été jugé
sans intérêt une fois ne doit pas revenir à chaque réimport de la même carte.

### `imports`

Trace de chaque passe d'indexation : dossier source, date, nombre de fichiers vus, ajoutés,
ignorés, en erreur. Sert au diagnostic, et à refaire une passe en confiance.

---

## 4. Rangement et indexation

### Arborescence

```
<racine>/
  Coulée du ruisseau/   ← un dossier par caméra
  Chablis nord/
  Mare basse/
```

Le dossier racine est configuré une fois. **Un dossier de premier niveau = un piège** :
le nom du dossier est rattaché à un `trap` à sa première rencontre, et tout ce qu'il
contient (y compris en sous-dossiers, les cartes SD créant souvent leurs propres niveaux)
hérite de ce piège. Plus aucune question de rattachement à l'import.

Un dossier inconnu apparaît comme tel dans le compte rendu et attend que je le rattache
à un piège existant ou que j'en crée un — jamais de piège créé en silence sur une faute
de frappe dans un nom de dossier.

### Règle fondatrice

**Toute vidéo présente sous la racine et absente de la base est en attente de
catégorisation.** La file de travail n'est pas une liste que l'application entretient :
c'est le résultat d'une comparaison entre le disque et la base, recalculée à chaque passe.
Rien ne peut donc être « oublié » : déposer des fichiers dans un dossier de caméra suffit
à les mettre dans la file.

### Passe d'indexation

1. Parcours récursif de la racine, extensions vidéo connues (`.mp4`, `.avi`, `.mov`, `.mkv`).
2. Pour chaque fichier : empreinte, puis `ffprobe` pour date, durée, dimensions, fps.
3. Fichier déjà connu par son empreinte → ignoré ; chemin changé → **chemin mis à jour**.
4. Empreinte présente dans `purged_videos` → **ré-ignoré** (voir §6).
5. Vignette extraite par ffmpeg (une image à ~1 s, plus une planche de 4 images
   pour la grille), stockée dans les données de l'application.
6. Rattachement au piège par le dossier de premier niveau.
7. Regroupement en séquences.
8. **Contrôle inverse** : toute vidéo de la base dont le fichier a disparu passe en
   `missing` (§6).
9. Compte rendu : *n* ajoutées, *n* déjà connues, *n* disparues, *n* dossiers inconnus,
   *n* sans date exploitable, *n* en erreur.

**Une vidéo sans date exploitable n'est jamais silencieusement datée d'aujourd'hui.**
Elle est marquée « date manquante » et attend une saisie manuelle — sur un fichier
d'archive, une fausse date est bien pire qu'une date absente.

ffmpeg et ffprobe sont **embarqués dans l'application**, pas supposés installés sur la machine.

---

## 5. Dépouillement

Deux vues, la même donnée.

**Dépouillement.** Les passages, **en tuiles ou en liste** — deux façons de voir la même
chose, pas deux écrans. Les tuiles servent au tri grossier : vider les fausses déclenches
d'un lot de 400 fichiers tient en quelques minutes, et survoler une tuile fait défiler les
vignettes de ses vidéos. La liste sert à lire les détails et porte le découpage :
regrouper, scinder à une vidéo donnée, fusionner. Sélection multiple et annotation en masse
dans les deux modes. **La taille des vignettes est réglable**, et le réglage est retenu.

Une leçon d'usage : un écran « Séquences » distinct de la grille a existé, et il faisait
doublon — deux vues des mêmes objets, avec des actions réparties entre les deux sans
raison. Fondu en un seul écran à deux modes d'affichage.

**Plein écran.** Une séquence à la fois, lecture en boucle, enchaînement automatique des
vidéos du groupe. Tout se fait au clavier : touches d'espèce configurables, `1`/`2`/`3`
pour la confiance, `0` pour « rien », `Entrée` pour valider et passer à la suite,
`←`/`→` pour naviguer, `Espace` pour la lecture. C'est la vue du dépouillement fin.

Dans les deux : scinder une séquence à la vidéo courante, fusionner avec la précédente.

---

## 6. Supprimer une vidéo

Trois cas, deux comportements. La distinction porte sur **ce qu'on garde**, jamais sur
la façon dont le fichier part.

### a. Supprimer sans laisser de trace — la fausse déclenche

Herbe qui bouge, pluie, rien à l'image, aucune information à conserver.
Le fichier part **et** la ligne de la base part.

Deux garde-fous, parce que c'est la seule opération vraiment irréversible :

- le fichier est mis à la **corbeille du système**, pas effacé — récupérable tant que je
  n'ai pas vidé la corbeille ;
- l'empreinte est enregistrée dans **`purged_videos`** (`content_hash`, `file_name`,
  `purged_at`). Sans cela, réimporter la même carte SD — que je n'efface pas forcément —
  ferait **revenir toutes les fausses déclenches déjà écartées**, une par une, à chaque passe.

### b. Supprimer en gardant la trace — le passage humain

Un promeneur, un chien, un tracteur : l'information compte pour les statistiques, la vidéo
non. Le fichier va à la corbeille, **la ligne reste**, avec tout ce qui permet d'analyser :
nom de fichier, piège, date et heure, durée, séquence, espèce ou état, tags, notes.

`file_state` passe à `purged`, `file_removed_at` et `file_removed_reason` sont renseignés.
La vidéo reste comptée dans toutes les statistiques ; seule la lecture est impossible, et
l'interface l'indique par une tuile grisée plutôt que par une vignette cassée.

C'est ici l'usage courant : **par défaut, dépouiller c'est garder la trace.** Le cas (a)
est réservé à ce qui n'a rien à dire.

### c. Fichier disparu hors de l'application

Vidéo supprimée à la main dans le Finder, disque débranché, dossier renommé.
La passe d'indexation le détecte (étape 8) et bascule la ligne en `missing` — ce qui est
**fonctionnellement le cas (b)** : la donnée reste, la vidéo est injouable.

La seule différence est l'intention, donc la réversibilité : un `missing` **redevient
`present` tout seul** si le fichier réapparaît, reconnu par son empreinte même sous un
autre nom ou un autre chemin. Un disque externe débranché ne détruit donc rien.

Une vidéo `missing` **jamais catégorisée** n'est ni de la donnée ni du travail en attente :
elle est signalée dans le compte rendu et proposée à la suppression définitive, sans être
supprimée d'office.

### Ce que l'application ne fait jamais

Vider la corbeille, supprimer un fichier sans que je l'aie demandé pour celui-là,
ou supprimer quoi que ce soit hors du dossier racine configuré.
Aucune suppression automatique, aucune règle de nettoyage.
---

## 7. Statistiques et recherche

**Rythme d'activité.** Distribution horaire par espèce, en heure locale *et* **repositionnée
par rapport au lever et au coucher du soleil** — c'est la référence pertinente pour un animal,
et un décalage d'une heure entre juin et décembre fausse toute lecture en heure civile.
Calcul solaire local à partir des coordonnées du piège, sans appel réseau.
Les séquences longues apparaissent pour ce qu'elles sont : une plage d'activité continue,
pas un pic de comptage.

**Saisonnalité.** Le **filtre par mois toutes années confondues** de Spoor (`months=12,1,2`) :
comparer le même mois d'une année sur l'autre est le cœur de l'analyse long terme.

**Comparaison entre emplacements.** Espèces présentes par piège, richesse spécifique,
répartition. ⚠️ **En comptages bruts uniquement** : sans effort de piégeage enregistré (§8),
deux pièges ne sont pas comparables si l'un a tourné trois mois et l'autre trois semaines.
La limite sera affichée dans l'interface plutôt que tue.

**Vue vidéo.** Une ligne par fichier, avec ses propres critères : état du fichier
(lisible / supprimée / disparue), sans date, sans position, et un tri chronologique dans
les deux sens. Différence qui compte : **les filtres de date et d'heure portent sur la
vidéo**, pas sur le début de son passage — chercher « entre 2 h et 3 h » doit rendre les
déclenchements de cette tranche, et non les passages commencés avant minuit. Une vidéo
hors séquence (sans date exploitable) y reste visible, là où les vues par séquence la
laissent forcément de côté.

**Recherche.** Filtres combinables : pièges, espèces (OU), tags (ET), confiance minimale,
plage de dates, mois toutes années, plage horaire, jour/nuit, durée de séquence,
état (`empty`, non dépouillé…), texte libre sur les notes.

**Exports.** CSV des séquences et des détections, et copie d'une sélection de fichiers
vers un dossier — la seule opération où l'application écrit des vidéos, et elle copie,
elle ne déplace pas.

---

## 8. Interface

Conventions Spoor, reprises telles quelles parce qu'elles ont fait leurs preuves :

- variables CSS dans `:root` (`--bg --panel --ink --muted --accent --border --danger`),
  thème via `[data-theme]` avec repli `prefers-color-scheme`, script anti-flash ;
- taille d'affichage réglable (`--ui-scale`) ;
- icônes monochromes maison, aucun emoji dans le chrome ;
- boîtes de dialogue sur `<dialog>` natif, jamais `window.confirm`, focus sur « Annuler » ;
- panneau d'édition en colonne ancrée, le contenu n'est jamais démonté au repli.

Pas de responsive mobile : l'application est de bureau, sur un grand écran.

---

## 9. Hors périmètre V1, et pourquoi

- **Effort de piégeage et taux pour 100 nuits.** Écarté explicitement. La conséquence est
  assumée : pas de comparaison honnête entre deux pièges inégalement exposés (§6).
  Rien dans le modèle n'empêche d'ajouter une table `deployments` ensuite.
- **Détection automatique** (MegaDetector pour animal/humain/véhicule/vide, puis espèce).
  Le modèle réserve la place : les colonnes d'identification automatique cohabiteront avec
  l'identification manuelle, cette dernière primant à l'affichage — exactement la relation
  `weather` / `weather_manual` de Spoor. On ne jette jamais la donnée d'origine.
- **OCR du bandeau incrusté.** Le bandeau porte date, heure, **nom de la caméra** et
  **température**. Les deux dernières ne sont nulle part ailleurs : la caméra donne le
  rattachement au piège, la température est une vraie donnée écologique. À faire quand le
  reste tournera ; d'ici là, `traps.camera_name` et un champ `temperature_c` nullable
  attendent leur source.
- **Hors-ligne, synchronisation, multi-appareils.** Sans objet : tout est local.
- ~~**Carte.**~~ Faite : Leaflet et fonds OpenStreetMap, pour poser un point au clic dans
  le formulaire de piège et dans l'ajustement des positions. C'est la seule fonction qui
  demande une connexion, et son absence n'empêche pas de saisir des coordonnées.
- **Pont vers Spoor.** Identifiants d'espèces et niveaux de confiance alignés dès maintenant
  pour que la passerelle soit simple le jour où elle sera utile.

---

## 10. Étapes de construction

1. Squelette Tauri + React/TS + SQLite, migrations au démarrage, thème et conventions CSS.
2. Indexation : parcours, empreinte, `ffprobe`, vignettes ffmpeg, compte rendu d'import.
3. Pièges et référentiel d'espèces, avec liste initiale.
4. Regroupement en séquences, scission et fusion manuelles.
5. Vue grille et annotation en masse.
5b. Suppressions : corbeille système, trace conservée, `purged_videos`, détection des `missing`.
6. Mode plein écran au clavier.
7. Recherche et filtres.
8. Statistiques : rythme d'activité solaire, saisonnalité, comparaison entre pièges.
9. Exports CSV et copie de sélection.
