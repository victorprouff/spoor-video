-- 005 — position propre à chaque vidéo.
--
-- Jusqu'ici la position d'une capture était lue sur son piège. Conséquence : déplacer
-- un piège déplaçait **rétroactivement** toutes les vidéos déjà tournées, y compris
-- celles d'il y a deux ans. Le rythme solaire et la comparaison entre emplacements
-- s'en trouvaient réécrits sans que rien ne le signale.
--
-- Désormais la position est **copiée sur la vidéo au moment de l'indexation**, depuis
-- le piège tel qu'il est ce jour-là. Le piège donne la valeur par défaut ; la vidéo
-- garde la sienne. Déplacer un piège n'a donc plus d'effet sur le passé.
--
-- C'est le même principe que `recorded_at` / `recorded_at_manual` : la donnée d'origine
-- et la correction cohabitent, et rien n'est réécrit dans le dos.

ALTER TABLE videos ADD COLUMN latitude REAL;
ALTER TABLE videos ADD COLUMN longitude REAL;
ALTER TABLE videos ADD COLUMN altitude_m REAL;

-- Vrai dès que la position a été choisie à la main : une passe d'indexation ne doit
-- jamais l'écraser avec celle du piège.
ALTER TABLE videos ADD COLUMN position_manual INTEGER NOT NULL DEFAULT 0;

-- Les vidéos déjà indexées prennent la position actuelle de leur piège : c'est
-- exactement ce que l'application utilisait jusqu'à présent, donc rien ne change
-- pour elles.
UPDATE videos
   SET latitude = (SELECT t.latitude FROM traps t WHERE t.id = videos.trap_id),
       longitude = (SELECT t.longitude FROM traps t WHERE t.id = videos.trap_id),
       altitude_m = (SELECT t.altitude_m FROM traps t WHERE t.id = videos.trap_id)
 WHERE latitude IS NULL;

CREATE INDEX idx_videos_position ON videos (latitude, longitude)
    WHERE deleted_at IS NULL AND latitude IS NOT NULL;
