-- 006 — suppression des tags.
--
-- Ils avaient été repris de Spoor par mimétisme, sans avoir été demandés ici. Sur un
-- carnet de terrain multi-usages ils servent ; sur un dépouillement de pièges photo,
-- l'espèce, l'état et les notes couvraient déjà le besoin. Un axe de classement qui ne
-- sert pas est un axe qu'il faut quand même comprendre, remplir et filtrer.
--
-- La suppression est franche : ces tables ne contenaient qu'un essai.

DROP TABLE IF EXISTS sequence_tags;
DROP TABLE IF EXISTS tags;
