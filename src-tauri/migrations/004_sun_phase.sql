-- 004 — position du passage par rapport au soleil.
--
-- Deux décalages distincts, qu'il ne faut pas confondre :
--
--   * `clock_offset_minutes` (migration 003) corrige une horloge de caméra FAUSSE ;
--   * `utc_offset_minutes`, ici, dit à quel fuseau correspond l'heure que la caméra
--     écrit — +60 pour la France en hiver, +120 en été.
--
-- Le second ne sert qu'au calcul solaire : sans lui, on comparerait un midi local
-- à un midi UTC, et tout le rythme d'activité serait décalé d'une heure ou deux.
--
-- Limite assumée : c'est une valeur fixe, pas un fuseau avec changements d'heure.
-- Une caméra qui suit l'heure d'été sera donc décrite à une heure près la moitié de
-- l'année. La conséquence se limite aux passages survenus juste au crépuscule.
ALTER TABLE traps ADD COLUMN utc_offset_minutes INTEGER NOT NULL DEFAULT 60;

-- Position du passage par rapport au soleil, calculée au regroupement depuis les
-- coordonnées du piège. NULL quand le piège n'a pas de position : on ne devine pas.
ALTER TABLE sequences ADD COLUMN sun_phase TEXT;

-- Minutes écoulées depuis le coucher du soleil (négatif avant). C'est la mesure qui
-- compte pour un animal : un renard sort « au crépuscule », pas « à 18 h ». En heure
-- civile, la même habitude semble se déplacer de plusieurs heures entre juin et
-- décembre.
ALTER TABLE sequences ADD COLUMN minutes_from_sunset INTEGER;

CREATE INDEX idx_sequences_sun ON sequences (sun_phase) WHERE deleted_at IS NULL;
