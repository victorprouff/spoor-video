-- 001 — schéma initial. Voir spec.md §3.
-- Conventions Spoor : identifiants UUID générés côté application, created_at/updated_at,
-- soft delete (deleted_at), donnée extraite et correction manuelle côté à côté.
-- Toutes les dates sont stockées en TEXT ISO-8601 UTC.

-- Les pièges : lieu durable. Un piège déplacé est un nouveau piège.
CREATE TABLE traps (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  folder_name   TEXT UNIQUE,          -- dossier de premier niveau sous la racine (§4)
  camera_name   TEXT,                 -- nom tel qu'écrit dans le bandeau incrusté
  latitude      REAL,
  longitude     REAL,
  altitude_m    REAL,
  notes         TEXT,
  active        INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  deleted_at    TEXT
);
CREATE INDEX idx_traps_folder ON traps (folder_name) WHERE deleted_at IS NULL;

-- Les séquences : le passage. Une séquence longue reste une séquence longue.
CREATE TABLE sequences (
  id            TEXT PRIMARY KEY,
  trap_id       TEXT NOT NULL REFERENCES traps (id),
  started_at    TEXT NOT NULL,
  ended_at      TEXT NOT NULL,
  video_count   INTEGER NOT NULL DEFAULT 0,
  auto_grouped  INTEGER NOT NULL DEFAULT 1,
  state         TEXT,                 -- NULL = non dépouillée ; sinon empty/unidentified/human/vehicle/livestock/species
  notes         TEXT,
  reviewed_at   TEXT,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  deleted_at    TEXT,
  CHECK (state IS NULL OR state IN ('species','empty','unidentified','human','vehicle','livestock'))
);
CREATE INDEX idx_sequences_trap_time ON sequences (trap_id, started_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_sequences_started ON sequences (started_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_sequences_unreviewed ON sequences (started_at) WHERE deleted_at IS NULL AND reviewed_at IS NULL;

-- Les vidéos : un fichier par ligne.
CREATE TABLE videos (
  id                  TEXT PRIMARY KEY,
  file_path           TEXT NOT NULL,
  file_name           TEXT NOT NULL,
  file_size           INTEGER,
  content_hash        TEXT NOT NULL UNIQUE,
  trap_id             TEXT NOT NULL REFERENCES traps (id),
  sequence_id         TEXT REFERENCES sequences (id),
  recorded_at         TEXT,           -- extrait de ffprobe ; NULL = date manquante, jamais devinée
  recorded_at_manual  TEXT,           -- correction ; recorded_at n'est jamais écrasé
  duration_s          REAL,
  width               INTEGER,
  height              INTEGER,
  fps                 REAL,
  thumbnail_path      TEXT,
  file_state          TEXT NOT NULL DEFAULT 'present',
  file_removed_at     TEXT,
  file_removed_reason TEXT,
  temperature_c       REAL,           -- attend l'OCR du bandeau (§9)
  imported_at         TEXT NOT NULL,
  reviewed_at         TEXT,
  created_at          TEXT NOT NULL,
  updated_at          TEXT NOT NULL,
  deleted_at          TEXT,
  CHECK (file_state IN ('present','purged','missing'))
);
CREATE INDEX idx_videos_sequence ON videos (sequence_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_videos_trap_time ON videos (trap_id, recorded_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_videos_path ON videos (file_path);
CREATE INDEX idx_videos_state ON videos (file_state) WHERE deleted_at IS NULL;

-- Référentiel d'espèces : liste courte locale, extensible.
CREATE TABLE species (
  id              TEXT PRIMARY KEY,
  common_name     TEXT NOT NULL UNIQUE,   -- français, fait foi
  scientific_name TEXT,
  species_group   TEXT NOT NULL DEFAULT 'autre',
  color           TEXT,
  sort_order      INTEGER NOT NULL DEFAULT 0,
  shortcut_key    TEXT,
  builtin         INTEGER NOT NULL DEFAULT 0,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  deleted_at      TEXT,
  CHECK (species_group IN ('mammifere','oiseau','autre'))
);
CREATE UNIQUE INDEX idx_species_shortcut ON species (shortcut_key) WHERE shortcut_key IS NOT NULL AND deleted_at IS NULL;

-- Plusieurs espèces par séquence, chacune avec SA confiance.
CREATE TABLE sequence_species (
  sequence_id  TEXT NOT NULL REFERENCES sequences (id) ON DELETE CASCADE,
  species_id   TEXT NOT NULL REFERENCES species (id),
  confidence   TEXT NOT NULL DEFAULT 'certain',
  count_min    INTEGER,
  count_max    INTEGER,
  notes        TEXT,
  created_at   TEXT NOT NULL,
  PRIMARY KEY (sequence_id, species_id),
  CHECK (confidence IN ('certain','probable','possible'))
);
CREATE INDEX idx_sequence_species_species ON sequence_species (species_id);

-- Tags libres : filtrent en ET. Pas de catégories de tags (détour déjà payé dans Spoor).
CREATE TABLE tags (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT
);
CREATE TABLE sequence_tags (
  sequence_id TEXT NOT NULL REFERENCES sequences (id) ON DELETE CASCADE,
  tag_id      TEXT NOT NULL REFERENCES tags (id),
  created_at  TEXT NOT NULL,
  PRIMARY KEY (sequence_id, tag_id)
);

-- Suppressions définitives (§6 cas a) : garde la mémoire d'avoir supprimé, pour que
-- réimporter la même carte SD ne fasse pas revenir les fausses déclenches écartées.
CREATE TABLE purged_videos (
  content_hash TEXT PRIMARY KEY,
  file_name    TEXT NOT NULL,
  purged_at    TEXT NOT NULL
);

-- Trace de chaque passe d'indexation.
CREATE TABLE imports (
  id              TEXT PRIMARY KEY,
  root_path       TEXT NOT NULL,
  started_at      TEXT NOT NULL,
  finished_at     TEXT,
  files_seen      INTEGER NOT NULL DEFAULT 0,
  files_added     INTEGER NOT NULL DEFAULT 0,
  files_known     INTEGER NOT NULL DEFAULT 0,
  files_repurged  INTEGER NOT NULL DEFAULT 0,
  files_missing   INTEGER NOT NULL DEFAULT 0,
  files_no_date   INTEGER NOT NULL DEFAULT 0,
  files_error     INTEGER NOT NULL DEFAULT 0,
  unknown_folders TEXT,
  created_at      TEXT NOT NULL
);

-- Réglages de l'application (racine des vidéos, écart de regroupement, thème…).
CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value      TEXT,
  updated_at TEXT NOT NULL
);
