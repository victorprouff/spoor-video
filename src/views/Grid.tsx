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
import { formatDateTime } from '../format';
import { Review } from './Review';

const DEFAULT_FILTER: GridFilter = {
  trap_id: null,
  review: 'unreviewed',
  states: [],
  species: [],
  tags: [],
  limit: 200,
  offset: 0,
};

function duration(seconds: number): string {
  if (seconds <= 0) return 'instantané';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h} h ${String(m).padStart(2, '0')}`;
  if (m > 0) return `${m} min`;
  return `${seconds} s`;
}

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

  const set = <K extends keyof GridFilter>(key: K, value: GridFilter[K]) =>
    setFilter({ ...filter, [key]: value, offset: 0 });

  return (
    <div className="stack grid-view">
      <section className="panel stack">
        <div className="row row--flush filters">
          <select value={filter.trap_id ?? ''} onChange={(e) => set('trap_id', e.target.value || null)}>
            <option value="">Tous les pièges</option>
            {traps.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>

          <select
            value={filter.review}
            onChange={(e) => set('review', e.target.value as GridFilter['review'])}
          >
            <option value="unreviewed">À dépouiller</option>
            <option value="reviewed">Dépouillées</option>
            <option value="all">Toutes</option>
          </select>

          <MultiSelect
            label="Espèces"
            options={species.map((s) => ({ value: s.id, label: s.common_name }))}
            selected={filter.species}
            onChange={(v) => set('species', v)}
            hint="Plusieurs espèces : l’une OU l’autre"
          />

          <MultiSelect
            label="Tags"
            options={tags.map((t) => ({ value: t.name, label: `${t.name} (${t.usage_count})` }))}
            selected={filter.tags}
            onChange={(v) => set('tags', v)}
            hint="Chaque tag ajouté affine (ET)"
          />

          <span className="app__spacer" />
          <span className="muted small">
            {page ? `${page.total} séquence(s)` : '…'}
            {page && page.unreviewed_total > 0 && ` · ${page.unreviewed_total} à dépouiller`}
          </span>
          <button onClick={() => setFilter(DEFAULT_FILTER)}>Réinitialiser</button>
        </div>
      </section>

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
            {duration(tile.duration_s)}
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
        <span className="muted small">{tile.trap_name}</span>
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

function MultiSelect({
  label,
  options,
  selected,
  onChange,
  hint,
}: {
  label: string;
  options: { value: string; label: string }[];
  selected: string[];
  onChange: (v: string[]) => void;
  hint: string;
}) {
  const [open, setOpen] = useState(false);
  if (options.length === 0) return null;

  return (
    <span className="multi">
      <button onClick={() => setOpen(!open)} className={selected.length ? 'tab--on' : undefined}>
        {label}
        {selected.length > 0 && ` (${selected.length})`}
      </button>
      {open && (
        <div className="multi__menu">
          <p className="muted small">{hint}</p>
          {options.map((o) => (
            <label key={o.value} className="multi__item">
              <input
                type="checkbox"
                checked={selected.includes(o.value)}
                onChange={(e) =>
                  onChange(
                    e.target.checked
                      ? [...selected, o.value]
                      : selected.filter((v) => v !== o.value),
                  )
                }
              />
              {o.label}
            </label>
          ))}
        </div>
      )}
    </span>
  );
}
