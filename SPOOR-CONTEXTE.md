# Spoor — contexte pour l'application sœur

> Document de contexte, à déposer à la racine du repo de la nouvelle application
> (catégorisation de vidéos de pièges photographiques). Il résume **ce qu'est Spoor,
> ses choix techniques et ses fonctionnalités**, et ce qui mérite d'être repris ou
> délibérément écarté. Les deux applications ne partagent aucun code aujourd'hui.
>
> Source de vérité côté Spoor : `spec.md`, `README.md`, `todo.md`, `tuiles.md`,
> `donnees-externes.md` dans `~/code/spoor`.

---

## 1. Ce qu'est Spoor

Carnet de terrain pour la photographie animalière. On note **où** et **quand** on a vu
quelque chose (animal, empreinte, crotte, terrier…), avec photos, météo et carte, et on
capitalise cette connaissance du terrain saison après saison.

- Usage **desktop** : saisie confortable, relecture, analyse, cartographie.
- Usage **mobile terrain** : capture rapide d'un point GPS + note + photo, **souvent sans réseau**.
- Démarré mono-utilisateur, aujourd'hui **multi-utilisateurs avec inscription publique**, en
  production sur `spoor.victorprouff.fr`.

**Philosophie imposée par l'auteur, et qui a tenu : léger et simple, pas de sur-ingénierie.**
Le VPS est modeste ; la cible est **2 conteneurs** (Postgres/PostGIS + une API Node qui sert
aussi le front statique), ~150–300 Mo de RAM au repos. Supabase a été explicitement écarté
(13 conteneurs, 1,5–4 Go idle).

---

## 2. Architecture

| Composant | Choix |
|---|---|
| Monorepo | **npm workspaces** (pas pnpm) — `apps/api`, `apps/web`, `apps/site` |
| Front | **PWA React 18 + TypeScript + Vite**, `vite-plugin-pwa` |
| Carte | **MapLibre GL JS** + tuiles **PMTiles auto-hébergées** (OSM), relief maison |
| Backend | **Node + TypeScript + Hono**, `pg` en **SQL brut (pas d'ORM)**, `zod` en validation |
| Base | **PostgreSQL + PostGIS**, `GEOGRAPHY(Point,4326)` + index GIST |
| Auth | **JWT maison** : `@node-rs/argon2` + `jose`. Pas de service d'auth tiers |
| Photos | **Volume Docker local**, servies par l'API avec vérification JWT (pas de S3) |
| Offline | **Dexie / IndexedDB** côté client, file d'attente maison |
| i18n | **i18next / react-i18next**, FR + EN, le **français fait foi** (typage des clés) |
| E-mail | **nodemailer** (SMTP) |
| Images serveur | **sharp** (redimensionnement, WebP) + **exifr** (EXIF) |
| Infra | **Dokploy** sur VPS, **déploiement auto au push sur `main`** |
| CSS | **CSS pur** dans un `styles.css`, variables CSS, **aucun framework CSS** |
| Tests | **Vitest** côté web uniquement, **aucun ESLint** |

Une seule origine : l'API sert le front statique → **pas de CORS**. HTTPS obligatoire partout
(Geolocation API et Service Worker l'exigent).

**Réécriture Rust du backend** : envisagée en V2+ « uniquement si un besoin réel de perf
apparaît ». Elle n'est jamais devenue nécessaire.

---

## 3. Modèle de données (les décisions qui comptent)

Table centrale `observations` ; `users`, `tags`, `photos`, `species`, `sites` autour.
22 migrations SQL numérotées, appliquées **au boot de l'API**.

Décisions structurantes, toutes réutilisables ailleurs :

1. **UUID généré côté client.** C'est la clé de tout le hors-ligne : une observation créée en
   mode avion a déjà son identifiant définitif, donc la sync est un `INSERT … ON CONFLICT (id)
   DO UPDATE` **idempotent**, rejouable même après un crash en pleine synchro.
2. **`observed_at` ≠ `created_at`.** Quand la chose a été vue (éditable) vs quand la note a été
   saisie (immuable). La saisie différée — le soir, pour une sortie du matin — est un cas
   d'usage central, pas un cas limite. **À reprendre tel quel pour des vidéos** : date de la
   capture ≠ date du dépouillement.
3. **Soft delete (`deleted_at`) obligatoire** dès qu'il y a du hors-ligne : une suppression doit
   être propagée aux clients, pas juste disparaître.
4. **`updated_at` + last-write-wins.** Pas de CRDT, pas de framework de sync : le serveur
   n'applique que si la version entrante est plus récente. Suffisant, et compris.
5. **Données brutes et corrections séparées** : `weather` (réponse API) et `weather_manual`
   (corrections terrain) cohabitent, la correction primant **champ par champ** à l'affichage.
   On ne jette jamais la donnée d'origine. Transposable à toute donnée auto-extraite d'une vidéo.
6. **`GEOGRAPHY` et pas `GEOMETRY`** : distances en mètres réels, `ST_DWithin(...)` direct.
7. **Lieu ≠ événement.** Erreur vécue et corrigée : `observations` mélangeait les deux, si bien
   que revenir trois fois au même terrier semait trois points à quelques mètres. Un **site**
   (lieu durable, nommé) porte désormais des **passages** (observations datées).
   **C'est exactement le modèle d'un piège photo** : l'emplacement du piège est durable, chaque
   vidéo est un événement daté rattaché à ce lieu. À reprendre dès le départ dans la nouvelle app.
8. **Type ≠ espèce ≠ tag.** Trois axes distincts, longtemps confondus dans un seul champ « tags » :
   - **type** d'observation (`tags.kind='type'`) : coloré, ordonné par priorité, donne sa couleur
     au point sur la carte — filtre en **OU** ;
   - **espèce** : référentiel taxonomique propre (`species`, `observation_species`), **plusieurs
     espèces par observation**, chacune avec **sa** confiance — filtre en **OU** ;
   - **tag** : mot-clé libre — filtre en **ET** (chaque tag ajouté affine).
   Les catégories de tags ont été créées puis **supprimées** : elles n'apportaient rien une fois
   les espèces sorties des tags. Ne pas refaire ce détour.
9. **Confiance d'identification à 3 niveaux** — certain / probable / possible. Pas de note sur 10 :
   une échelle fine donne une fausse précision et personne ne sait choisir entre 6 et 7.

---

## 4. API

REST, préfixe `/api/v1/`, JSON, `Authorization: Bearer <JWT>`.

- Auth : `login`, `refresh`, `register`, `verify-email`, `resend-verification`,
  `forgot-password`, `reset-password`.
- `observations` (CRUD + upsert idempotent), `tags`, `sites` (+ `merge`), `species`,
  `photos` (multipart, `/file` et `/thumb` sous JWT), `weather:fetch`, `feedback`,
  `coverage-request`, `admin/users`, `account`.
- **`GET /sync?since=<ts>`** : le delta (créé / modifié / soft-deleted depuis). Une seule route,
  tout le hors-ligne repose dessus.
- Filtres riches sur `GET /observations` : `tags` (ET), `types` (OU), `species` (OU),
  `confidence_min`, `near=lat,lng&radius_m`, `bbox`, `from`/`to`, **`months=12,1,2`**
  (filtre saisonnier toutes années confondues — cœur de l'analyse long terme, très pertinent
  pour des habitudes filmées), `q` plein texte, pagination par curseur.

**Règles de sécurité des e-mails, apprises en prod et à recopier** : répondre **204 toujours**
(ne jamais révéler si une adresse existe), ne stocker que **l'empreinte** du jeton, usage unique,
cadence (rate limit), et **révoquer les sessions au changement de mot de passe** (comparaison
`password_changed_at` / `iat` du JWT). Piège vécu : **un SMTP absent ne se voit pas** — tout
répond 204. Prévoir l'alerte.

---

## 5. Hors-ligne et synchronisation

Philosophie : **serveur = source de vérité, client = cache + file d'attente.**

- Miroir partiel des données dans **Dexie** (aujourd'hui en v10, chaque évolution de modèle a
  son `upgrade`).
- Table `pendingOps` (`upsert` / `delete`), plus des files dédiées pour les **photos** (Blobs,
  uploadées une à une au retour du réseau, avec retry) et les **messages**.
- Au retour du réseau : rejeu ordonné de la file, puis `GET /sync?since=…`, puis nouveau curseur.
- Écriture locale = **transaction atomique** miroir + file (sinon incohérence après crash).
- **Carte hors-ligne** : protocole maison `spoor://` + magasin Dexie pour les tuiles.

Bug de fond vécu, à ne pas reproduire : le contexte d'auth effaçait le token sur **toute**
erreur de `GET /me` au démarrage, réseau compris → déconnexion en mode avion.
**Seul un 401 doit déconnecter** ; un token en cache est accepté immédiatement puis revalidé
en arrière-plan.

**Mises à jour de la PWA** : `registerType: 'prompt'` et **pas** `autoUpdate` — sur le terrain,
un rechargement décidé par l'app peut jeter une saisie en cours. Bandeau + pastille, et
vérification au retour au premier plan (throttlée), sans quoi une PWA installée reste des jours
sur du vieux code. Le pied du menu affiche **SHA du commit + date**.

---

## 6. Fonctionnalités livrées (en production)

**Saisie et terrain**
- Point GPS auto puis **affiné en arrière-plan** (fenêtre 60 s, on garde le meilleur relevé),
  précision **qualifiée et colorée** (bon ≤ 25 m, approximatif ≤ 100 m, au-delà « position
  réseau »), marqueur déplaçable avec annulation du déplacement.
- Notes, date/heure éditable, types, espèces multiples avec confiance, tags libres.
- Sites (lieux) et passages datés, suggestion de rattachement sous 25 m, fusion de doublons.
- **Photos** : galerie **et** appareil photo, EXIF → pré-remplissage date + position,
  réencodage **WebP** redimensionné à l'upload, miniatures 300 px, visionneuse plein écran,
  sélection multiple, upload hors-ligne différé.
- **Créer une observation depuis une photo** : l'EXIF donne date et souvent position,
  appliquées d'office, **annoncées, et annulables séparément**.
- **Météo automatique** via **Open-Meteo** (sans clé) : `forecast` pour le récent, `archive`
  pour l'historique — indispensable à la saisie différée — plus correction manuelle.

**Consultation et analyse**
- Carte MapLibre : clustering, couleur par type prioritaire, légende repliable, filtres
  (types / espèces / tags / dates / **mois toutes années**), recherche de lieu (Photon),
  résultats listés dans le panneau de filtres.
- **Fonds de carte multiples auto-hébergés** (PMTiles) + **relief maison** : estompage
  (`hillshade`) et **courbes de niveau calculées dans le navigateur** (`maplibre-contour`)
  depuis un MNT de 0,58 Go. Aucune dépendance externe à l'exécution.
- **Couverture géographique déclarative** : `scripts/zones.txt` est la source unique lue par la
  carte et par le relief ; ajouter une zone = une ligne + deux commandes.
- **Guide d'espèces** (`/fiches`) : recherche mondiale et « autour de moi », fiches construites
  depuis **iNaturalist / GBIF / Wikipédia**, sections utiles au terrain découpées côté serveur.

**Compte et exploitation**
- Inscription publique avec confirmation d'adresse, mot de passe oublié, CGU horodatées.
- Rôles (user / admin / super-admin, protégé par un trigger SQL), offre, **fonctions sous
  drapeau** (`user_features`) pour livrer une bêta à quelques comptes.
- Page « Compte » (ses chiffres, identité, mot de passe), page d'admin, « écrire à l'auteur ».
- **Site de présentation public** (`apps/site`), généré, multilingue, thème partagé avec l'app.
- Compte de démonstration + **captures d'écran toutes générées par script** (`demo-shots.mjs`).

---

## 7. Interface — conventions qui ont fait leurs preuves

- **Variables CSS** dans `:root` pour tout (`--bg --panel --ink --muted --accent --border
  --danger`), thème via `[data-theme]` avec repli `prefers-color-scheme`, `color-scheme` posé
  par thème, **script inline anti-flash** avant le rendu.
- **Taille d'affichage réglable** (`--ui-scale`), plus grande par défaut sur mobile.
- Breakpoints **jointifs** : `min-width: 768px` / `max-width: 767.98px`. Un trou 768/769
  laisse un écran sans aucune règle — bug vécu en paysage.
- **Sur mobile, ce qui agit descend en bas** (barre d'actions, bouton « ＋ » flottant dans la
  zone du pouce) ; l'information monte en haut.
- **Icônes monochromes maison**, plus aucun emoji dans le chrome ; logo à source unique.
- Boîtes de dialogue maison sur `<dialog>` natif, jamais `window.confirm` ; le focus va sur
  « Annuler », jamais sur l'action destructive.
- Panneau de saisie = **colonne ancrée** sur desktop (la carte se rétrécit, rien n'est
  recouvert), **feuille basse redimensionnable** sur mobile. Le contenu **n'est jamais démonté**
  au repli, sinon la saisie est perdue.

---

## 8. Déploiement et exploitation

- **Dokploy** sur VPS, domaine `spoor.victorprouff.fr`, TLS par Traefik.
- **Déploiement automatique à chaque push sur `main`** ; migrations appliquées au boot.
- Backups Dokploy sur **les deux volumes** : base **et** photos.
- ⚠️ **`dokploy-network` est partagé par tous les projets du VPS** et Compose y pose un alias
  égal au nom du service : deux projets avec un service `api` → DNS multi-adresses, une requête
  sur deux chez le mauvais projet. Parade retenue : réseau `internal` privé pour la base
  (alias `spoor-db`) et **alias unique `spoor-api`** pour l'API. **Donner d'emblée à la nouvelle
  application des noms de service uniques.**
- **Sonder la prod par le CONTENU, pas par le code HTTP** : le repli SPA renvoie 200 avec
  l'`index.html`. Le témoin fiable d'un déploiement front est l'empreinte du bundle.

---

## 9. Ce que la nouvelle application peut reprendre

À reprendre presque tel quel :
- le socle **monorepo npm workspaces + Hono + pg (SQL brut) + Postgres**, une image, deux
  conteneurs, déploiement Dokploy ;
- l'**auth JWT maison** et toute la mécanique e-mail (204 systématique, empreinte de jeton,
  révocation des sessions) ;
- le triptyque **UUID client + soft delete + LWW + `/sync?since=`** si le dépouillement doit
  marcher hors connexion ;
- **lieu durable / événement daté** : un piège photo est un site, chaque vidéo un passage ;
- **`observed_at` ≠ `created_at`**, le **filtre par mois toutes années** (habitudes et
  saisonnalité), la **confiance à 3 niveaux**, **plusieurs espèces par capture** ;
- les conventions CSS, thème, échelle, breakpoints et le jeu d'icônes.

À questionner avant de reprendre :
- **le hors-ligne intégral** : dépouiller des vidéos se fait plutôt au bureau qu'en forêt.
  C'est la partie la plus coûteuse de Spoor — ne la reprendre que si le besoin est réel ;
- **la carte** : centrale pour Spoor, probablement secondaire ici (quelques emplacements de
  pièges, pas des milliers de points) ;
- **le stockage** : des vidéos ne sont pas des photos. Un volume Docker local et `sharp` ne
  transposent pas ; il faudra trancher stockage, transcodage et extraction de vignettes.

À éviter, parce que déjà payé une fois :
- catégories de tags (détour inutile), note de confiance sur 10, `autoUpdate` de PWA,
  breakpoints non jointifs, déconnexion sur erreur réseau, noms de services Docker génériques.
