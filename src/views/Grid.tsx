import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

import {
  CONFIDENCES,
  STATES,
  annotateSequences,
  gridPage,
  listSpecies,
  listTags,
  listTraps,
} from '../api';
import type {
  Annotation,
  Confidence,
  GridFilter,
  GridPage,
  GridTile,
  Species,
  Tag,
  Trap,
} from '../api';
import { SUN_PHASES } from '../api';
import { EMPTY_FILTER, Filters } from '../components/Filters';
import { formatDateTime, formatDuration } from '../format';
import { Review } from './Review';

// Par défaut on montre ce qui reste à faire : c'est la raison d'ouvrir cet onglet.
const DEFAULT_FILTER: GridFilter = { ...EMPTY_FILTER, review: 'unreviewed' };

export function Grid({ onError }: { onError: (e: string | null) => void }) {
  const [filter, setFilter] = useState<GridFilter>(DEFAULT_FILTER);
  const [page, setPage] = useState<GridPage | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [species, setSpecies] = useState<Species[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const lastClicked = useRef<string | null>(null);
  const [reviewAt, setReviewAt] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setPage(await gridPage(filter));
    } catch (e) {
      onError(String(e));
    }
  }, [filter, onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    listTraps().then(setTraps).catch(() => undefined);
    listSpecies().then(setSpecies).catch(() => undefined);
    listTags().then(setTags).catch(() => undefined);
  }, [page]);

  const tiles = page?.tiles ?? [];

  /** Clic simple : sélection unique. Cmd : bascule. Maj : plage. */
  const click = (tile: GridTile, e: React.MouseEvent) => {
    const next = new Set(selected);
    if (e.shiftKey && lastClicked.current) {
      const from = tiles.findIndex((t) => t.id === lastClicked.current);
      const to = tiles.findIndex((t) => t.id === tile.id);
      if (from >= 0 && to >= 0) {
        const [a, b] = from < to ? [from, to] : [to, from];
        for (let i = a; i <= b; i++) next.add(tiles[i].id);
      }
    } else if (e.metaKey || e.ctrlKey) {
      if (next.has(tile.id)) next.delete(tile.id);
      else next.add(tile.id);
      lastClicked.current = tile.id;
    } else {
      next.clear();
      next.add(tile.id);
      lastClicked.current = tile.id;
    }
    setSelected(next);
  };

  const selectAll = () => setSelected(new Set(tiles.map((t) => t.id)));
  const clear = useCallback(() => setSelected(new Set()), []);

  useEffect(() => {
    // Le plein écran a son propre clavier : deux gestionnaires actifs ensemble
    // feraient qu'Échap vide la sélection en même temps qu'il ferme la vue.
    if (reviewAt !== null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') clear();
      if ((e.metaKey || e.ctrlKey) && e.key === 'a') {
        e.preventDefault();
        setSelected(new Set(tiles.map((t) => t.id)));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [tiles, clear, reviewAt]);

  const apply = async (annotation: Annotation) => {
    onError(null);
    try {
      await annotateSequences([...selected], annotation);
      clear();
      await refresh();
      setTags(await listTags());
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="stack grid-view">
      <Filters filter={filter} onChange={setFilter} traps={traps} species={species} tags={tags} />

      <div className="row row--flush">
        <span className="muted small">
          {page ? `${page.total} séquence(s)` : '…'}
          {page && page.unreviewed_total > 0 && ` · ${page.unreviewed_total} à dépouiller au total`}
        </span>
        <span className="app__spacer" />
        {page && page.total > filter.limit && (
          <>
            <button
              onClick={() => setFilter({ ...filter, offset: Math.max(0, filter.offset - filter.limit) })}
              disabled={filter.offset === 0}
            >
              Précédentes
            </button>
            <span className="muted small">
              {filter.offset + 1}–{Math.min(filter.offset + filter.limit, page.total)}
            </span>
            <button
              onClick={() => setFilter({ ...filter, offset: filter.offset + filter.limit })}
              disabled={filter.offset + filter.limit >= page.total}
            >
              Suivantes
            </button>
          </>
        )}
      </div>

      {tiles.length === 0 ? (
        <section className="panel">
          <p className="muted">
            Aucune séquence ne correspond. Lance une passe d’indexation, ou élargis les filtres.
          </p>
        </section>
      ) : (
        <section className="panel stack">
          <div className="row row--flush">
            <button className="primary" onClick={() => setReviewAt(0)}>
              Dépouiller en plein écran
            </button>
            <button onClick={selectAll}>Tout sélectionner</button>
            <button onClick={clear} disabled={selected.size === 0}>
              Désélectionner
            </button>
            <span className="muted small">Clic pour choisir · Maj : plage · ⌘ : ajouter</span>
          </div>
          <div className="tiles">
            {tiles.map((tile) => (
              <Tile
                key={tile.id}
                tile={tile}
                selected={selected.has(tile.id)}
                onClick={(e) => click(tile, e)}
                onOpen={() => setReviewAt(tiles.findIndex((t) => t.id === tile.id))}
              />
            ))}
          </div>
        </section>
      )}

      {selected.size > 0 && (
        <AnnotationBar count={selected.size} species={species} onApply={apply} onCancel={clear} />
      )}

      {reviewAt !== null && (
        <Review
          queue={tiles}
          startAt={reviewAt}
          species={species}
          onClose={() => {
            setReviewAt(null);
            void refresh();
          }}
          onChanged={() => undefined}
          onError={onError}
        />
      )}
    </div>
  );
}

/** Tuile d'une séquence. Survoler fait défiler les vignettes de ses vidéos :
    une planche de contact, sans générer d'image supplémentaire. */
function Tile({
  tile,
  selected,
  onClick,
  onOpen,
}: {
  tile: GridTile;
  selected: boolean;
  onClick: (e: React.MouseEvent) => void;
  onOpen: () => void;
}) {
  const [frame, setFrame] = useState(0);
  const timer = useRef<number | null>(null);

  const start = () => {
    if (tile.thumbnails.length < 2) return;
    timer.current = window.setInterval(
      () => setFrame((f) => (f + 1) % tile.thumbnails.length),
      600,
    );
  };
  const stop = () => {
    if (timer.current) window.clearInterval(timer.current);
    timer.current = null;
    setFrame(0);
  };
  useEffect(() => stop, []);

  const thumb = tile.thumbnails[frame];
  const stateLabel = STATES.find((s) => s.value === tile.state)?.label;
  const sunLabel = SUN_PHASES.find((p) => p.value === tile.sun_phase)?.label;

  return (
    <button
      type="button"
      className={selected ? 'tile tile--on' : 'tile'}
      onClick={onClick}
      onDoubleClick={onOpen}
      title="Double-clic : dépouiller en plein écran"
      onMouseEnter={start}
      onMouseLeave={stop}
    >
      <div className="tile__image">
        {thumb ? (
          <img src={convertFileSrc(thumb)} alt="" loading="lazy" />
        ) : (
          <div className="tile__none">pas de vignette</div>
        )}
        {tile.video_count > 1 && <span className="tile__count">{tile.video_count}</span>}
        {tile.duration_s >= 3600 && (
          <span className="tile__long" title="Activité continue">
            {formatDuration(tile.duration_s)}
          </span>
        )}
        {tile.unplayable_count > 0 && (
          <span className="tile__warn" title="Fichier supprimé ou disparu">
            ⌀
          </span>
        )}
      </div>
      <div className="tile__meta">
        <span className="small">{formatDateTime(tile.started_at)}</span>
        <span className="muted small">
          {tile.trap_name}
          {sunLabel && ` · ${sunLabel.toLowerCase()}`}
        </span>
        {tile.species.length > 0 && (
          <span className="tile__species">
            {tile.species.map((s) => (
              <span key={s.id} className={`chip chip--${s.confidence}`}>
                <span className="swatch" style={{ background: s.color ?? 'transparent' }} />
                {s.common_name}
              </span>
            ))}
          </span>
        )}
        {!tile.species.length && stateLabel && <span className="chip">{stateLabel}</span>}
        {tile.tags.length > 0 && (
          <span className="muted small">{tile.tags.map((t) => `#${t}`).join(' ')}</span>
        )}
      </div>
    </button>
  );
}

/** Barre d'annotation en masse : ce qui agit reste sous la main, en bas. */
function AnnotationBar({
  count,
  species,
  onApply,
  onCancel,
}: {
  count: number;
  species: Species[];
  onApply: (a: Annotation) => void;
  onCancel: () => void;
}) {
  const [speciesId, setSpeciesId] = useState('');
  const [confidence, setConfidence] = useState<Confidence>('certain');
  const [tagText, setTagText] = useState('');

  const tagList = useMemo(
    () =>
      tagText
        .split(',')
        .map((t) => t.trim())
        .filter(Boolean),
    [tagText],
  );

  const applySpecies = () => {
    if (!speciesId) return;
    onApply({
      add_species: [{ species_id: speciesId, confidence, count_min: null, count_max: null }],
      add_tags: tagList,
    });
    setTagText('');
  };

  return (
    <div className="actionbar">
      <strong>{count} sélectionnée(s)</strong>

      <select value={speciesId} onChange={(e) => setSpeciesId(e.target.value)}>
        <option value="">Espèce…</option>
        {species.map((s) => (
          <option key={s.id} value={s.id}>
            {s.common_name}
            {s.shortcut_key ? ` (${s.shortcut_key})` : ''}
          </option>
        ))}
      </select>

      <select value={confidence} onChange={(e) => setConfidence(e.target.value as Confidence)}>
        {CONFIDENCES.map((c) => (
          <option key={c.value} value={c.value}>
            {c.label}
          </option>
        ))}
      </select>

      <input
        placeholder="tags, séparés par des virgules"
        value={tagText}
        onChange={(e) => setTagText(e.target.value)}
      />

      <button className="primary" onClick={applySpecies} disabled={!speciesId}>
        Appliquer
      </button>

      <span className="actionbar__sep" />

      {STATES.map((s) => (
        <button key={s.value} onClick={() => onApply({ state: s.value, add_tags: tagList })}>
          {s.label}
        </button>
      ))}

      <span className="app__spacer" />
      <button onClick={() => onApply({ reviewed: false })}>À revoir</button>
      <button onClick={onCancel}>Annuler</button>
    </div>
  );
}
