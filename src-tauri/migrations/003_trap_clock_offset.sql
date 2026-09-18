-- 003 — décalage d'horloge par piège.
--
-- Les pièges photo écrivent dans le conteneur l'heure de leur horloge interne, qui
-- n'est ni forcément l'heure locale, ni forcément à l'heure. Un décalage se corrige
-- une fois **par piège**, pas vidéo par vidéo.
--
-- Suivant la règle du §3 : la donnée brute (`recorded_at`) n'est jamais réécrite,
-- le décalage est appliqué à la lecture. Une valeur fausse se corrige donc après coup
-- sans réindexer quoi que ce soit.

ALTER TABLE traps ADD COLUMN clock_offset_minutes INTEGER NOT NULL DEFAULT 0;
