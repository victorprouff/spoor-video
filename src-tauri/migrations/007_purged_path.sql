-- 007 — mémoriser aussi le chemin des vidéos écartées sans trace.
--
-- Défaut corrigé : la pastille « des vidéos attendent » comparait des chemins, sans
-- savoir qu'une vidéo peut avoir été écartée volontairement. Une vidéo supprimée sans
-- trace puis revenue sur le disque — restaurée depuis la corbeille, ou resynchronisée
-- par un client cloud quand la racine vit dans un dossier synchronisé — était comptée
-- comme nouvelle à chaque passage, sans jamais pouvoir l'être.
--
-- Le chemin permet à la pastille de reconnaître ces fichiers sans avoir à calculer leur
-- empreinte, c'est-à-dire sans les lire.

ALTER TABLE purged_videos ADD COLUMN file_path TEXT;

CREATE INDEX idx_purged_path ON purged_videos (file_path) WHERE file_path IS NOT NULL;
