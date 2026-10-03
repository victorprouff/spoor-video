-- 008 — vidéos favorites.
--
-- Une marque posée à la main sur un fichier, pour retrouver les belles captures. Elle
-- porte sur la **vidéo**, pas sur la séquence : dans un passage de cinq déclenchements,
-- c'est souvent un seul qui vaut d'être revu.
--
-- Une date plutôt qu'un booléen : NULL = pas favorite, et le jour où on la marque reste
-- connu, sans coût.

ALTER TABLE videos ADD COLUMN favorite_at TEXT;

CREATE INDEX idx_videos_favorite ON videos (favorite_at)
    WHERE deleted_at IS NULL AND favorite_at IS NOT NULL;
