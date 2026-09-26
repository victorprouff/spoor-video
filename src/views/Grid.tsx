import { useCallback, useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

import {
  CONFIDENCES,
  STATES,
  annotateSequences,
  gridPage,
  listSequenceVideos,
  listSpecies,
  listTraps,
  mergeSequences,
  regroupSequences,
  splitSequence,
} from '../api';
import type {
  Annotation,
  SequenceVideo,
  Confidence,
  GridFilter,
  GridPage,
  GridTile,
  Species,
  Trap,
} from '../api';
import { SUN_PHASES } from '../api';
import { DeleteDialog } from '../components/DeleteDialog';
import { Export } from '../components/Export';
import { SpeciesPicker } from '../components/SpeciesPicker';
import { ViewControls, useDisplay, useThumbSize } from '../components/ViewControls';
import { EMPTY_FILTER, Filters } from '../components/Filters';
import { formatDateTime, formatDuration } from '../format';
import { Review } from './Review';

// Par défaut on montre ce qui reste à faire : c'est la raison d'ouvrir cet onglet.
const DEFAULT_FILTER: GridFilter = { ...EMPTY_FILTER, review: 'unreviewed' };

export function Grid({
  onError,
  onChanged,
  focusSequenceId = null,
  onClearFocus,
}: {
  onError: (e: string | null) => void;
  /** Après chaque rafraîchissement : ce qui reste à dépouiller a pu changer. */
  onChanged?: () => void;
  /** Séquence rouverte depuis l'onglet Vidéos, pour la corriger. */
  focusSequenceId?: string | null;
  onClearFocus?: () => void;
}) {
  const [filter, setFilter] = useState<GridFilter>(() =>
    focusSequenceId ? { ...EMPTY_FILTER, sequence_id: focusSequenceId } : DEFAULT_FILTER,
  );

  useEffect(() => {
    setFilter(
      focusSequenceId ? { ...EMPTY_FILTER, sequence_id: focusSequenceId } : DEFAULT_FILTER,
    );
  }, [focusSequenceId]);
  const [page, setPage] = useState<GridPage | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [species, setSpecies] = useState<Species[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const lastClicked = useRef<string | null>(null);
  const [reviewAt, setReviewAt] = useState<number | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteNote, setDeleteNote] = useState<string | null>(null);
  const [display, setDisplay] = useDisplay('grid', 'tiles');
  const [thumbSize, setThumbSize] = useThumbSize('grid', 220);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setPage(await gridPage(filter));
      onChanged?.();
    } catch (e) {
      onError(String(e));
    }
  }, [filter, onError, onChanged]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    listTraps().then(setTraps).catch(() => undefined);
    listSpecies().then(setSpecies).catch(() => undefined);
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

  // --- Actions sur les séquences, rapatriées de l'ancienne vue Séquences ------

  const regroup = async () => {
    setBusy(true);
    onError(null);
    try {
      const r = await regroupSequences();
      clear();
      await refresh();
      setDeleteNote(
        `${r.sequences_built} séquence(s) reconstruite(s), ` +
          `${r.sequences_frozen} laissée(s) intacte(s)`,
      );
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const merge = async () => {
    onError(null);
    try {
      await mergeSequences([...selected]);
      clear();
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const split = async (sequenceId: string, videoId: string) => {
    onError(null);
    try {
      await splitSequence(sequenceId, videoId);
      setExpanded(null);
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  // Fusionner n'a de sens qu'entre séquences d'un même piège : on le dit avant
  // plutôt que de laisser le serveur refuser.
  const selectedTiles = tiles.filter((t) => selected.has(t.id));
  const sameTrap =
    selectedTiles.length > 1 && new Set(selectedTiles.map((t) => t.trap_id)).size === 1;

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
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="stack grid-view">
      <Filters filter={filter} onChange={setFilter} traps={traps} species={species} />

      {filter.sequence_id && (
        <div className="panel notice notice--new">
          <div className="row row--flush">
            <span>Une seule séquence, rouverte depuis l’onglet Vidéos pour la corriger.</span>
            <span className="app__spacer" />
            <button
              onClick={() => {
                setFilter(DEFAULT_FILTER);
                onClearFocus?.();
              }}
            >
              Voir tout ce qui reste à dépouiller
            </button>
          </div>
        </div>
      )}

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
            <button
              onClick={merge}
              disabled={!sameTrap}
              title={
                selectedTiles.length > 1 && !sameTrap
                  ? 'Ces séquences appartiennent à des pièges différents'
                  : 'Recoller plusieurs passages en un seul'
              }
            >
              Fusionner ({selectedTiles.length})
            </button>
            <button onClick={regroup} disabled={busy} title="Reconstruire les regroupements">
              {busy ? 'Regroupement…' : 'Regrouper'}
            </button>
            <span className="app__spacer" />
            <ViewControls
              display={display}
              onDisplay={setDisplay}
              size={thumbSize}
              onSize={setThumbSize}
            />
          </div>
          <p className="muted small">
            Clic pour choisir · Maj : plage · ⌘ : ajouter. Un regroupement ne défait jamais une
            séquence scindée, fusionnée ou déjà annotée.
          </p>
          {display === 'tiles' ? (
            <div className="tiles" style={{ ['--tile-w' as string]: `${thumbSize}px` }}>
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
          ) : (
            <table className="table">
              <thead>
                <tr>
                  <th />
                  <th>Début</th>
                  <th>Piège</th>
                  <th className="num">Vidéos</th>
                  <th>Durée</th>
                  <th>Espèces</th>
                  <th>État</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {tiles.map((tile) => (
                  <SequenceRow
                    key={tile.id}
                    tile={tile}
                    checked={selected.has(tile.id)}
                    expanded={expanded === tile.id}
                    thumbSize={thumbSize}
                    onToggleCheck={() => {
                      const next = new Set(selected);
                      if (next.has(tile.id)) next.delete(tile.id);
                      else next.add(tile.id);
                      setSelected(next);
                    }}
                    onToggleExpand={() => setExpanded(expanded === tile.id ? null : tile.id)}
                    onOpen={() => setReviewAt(tiles.findIndex((t) => t.id === tile.id))}
                    onSplit={(videoId) => split(tile.id, videoId)}
                    onError={onError}
                  />
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}

      <Export
        filter={filter}
        selection={[...selected]}
        total={page?.total ?? 0}
        onError={onError}
      />

      {selected.size > 0 && (
        <AnnotationBar
          count={selected.size}
          species={species}
          onApply={apply}
          onCancel={clear}
          onDelete={() => setDeleting(true)}
        />
      )}

      {deleting && (
        <DeleteDialog
          sequenceIds={[...selected]}
          onCancel={() => setDeleting(false)}
          onError={onError}
          onDone={(r, mode) => {
            setDeleting(false);
            clear();
            void refresh();
            const parts = [`${r.trashed} fichier(s) à la corbeille`];
            if (mode === 'purge') parts.push(`${r.rows_removed} ligne(s) supprimée(s)`);
            if (r.already_gone) parts.push(`${r.already_gone} déjà absent(s)`);
            if (r.sequences_removed) parts.push(`${r.sequences_removed} séquence(s) vidée(s)`);
            setDeleteNote(parts.join(' · '));
            if (r.errors.length) onError(r.errors.join('\n'));
          }}
        />
      )}

      {deleteNote && (
        <div className="panel notice">
          <p>
            {deleteNote}{' '}
            <span className="muted small">
              — récupérable dans la corbeille tant qu’elle n’est pas vidée.
            </span>{' '}
            <button className="small" onClick={() => setDeleteNote(null)}>
              Fermer
            </button>
          </p>
        </div>
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
  onDelete,
}: {
  count: number;
  species: Species[];
  onApply: (a: Annotation) => void;
  onCancel: () => void;
  onDelete: () => void;
}) {
  const [picked, setPicked] = useState<Map<string, Confidence>>(new Map());
  const [confidence, setConfidence] = useState<Confidence>('certain');
  const applySpecies = () => {
    if (picked.size === 0) return;
    onApply({
      add_species: [...picked].map(([species_id, conf]) => ({
        species_id,
        confidence: conf,
        count_min: null,
        count_max: null,
      })),
    });
    setPicked(new Map());
  };

  const toggle = (id: string) =>
    setPicked((p) => {
      const next = new Map(p);
      if (next.has(id)) next.delete(id);
      else next.set(id, confidence);
      return next;
    });

  return (
    <div className="actionbar">
      <strong>{count} sélectionnée(s)</strong>

      <SpeciesPicker
        species={species}
        selected={picked}
        onToggle={toggle}
        onSetConfidence={(id, c) => setPicked((p) => new Map(p).set(id, c))}
      />

      <select
        value={confidence}
        onChange={(e) => setConfidence(e.target.value as Confidence)}
        title="Confiance appliquée aux espèces ajoutées ensuite"
      >
        {CONFIDENCES.map((c) => (
          <option key={c.value} value={c.value}>
            {c.label}
          </option>
        ))}
      </select>

      <button className="primary" onClick={applySpecies} disabled={picked.size === 0}>
        Appliquer
      </button>

      <span className="actionbar__sep" />

      {STATES.map((s) => (
        <button key={s.value} onClick={() => onApply({ state: s.value })}>
          {s.label}
        </button>
      ))}

      <span className="app__spacer" />
      <button onClick={() => onApply({ reviewed: false })}>À revoir</button>
      <button className="destructive" onClick={onDelete}>
        Supprimer…
      </button>
      <button onClick={onCancel}>Annuler</button>
    </div>
  );
}


/** Une séquence en ligne, dépliable sur ses vidéos — l'ancienne vue Séquences,
    ramenée ici pour ne pas avoir deux écrans qui montrent la même chose. */
function SequenceRow({
  tile,
  checked,
  expanded,
  thumbSize,
  onToggleCheck,
  onToggleExpand,
  onOpen,
  onSplit,
  onError,
}: {
  tile: GridTile;
  checked: boolean;
  expanded: boolean;
  thumbSize: number;
  onToggleCheck: () => void;
  onToggleExpand: () => void;
  onOpen: () => void;
  onSplit: (videoId: string) => void;
  onError: (e: string | null) => void;
}) {
  const [videos, setVideos] = useState<SequenceVideo[] | null>(null);

  useEffect(() => {
    if (!expanded || videos) return;
    listSequenceVideos(tile.id)
      .then(setVideos)
      .catch((e) => onError(String(e)));
  }, [expanded, videos, tile.id, onError]);

  const stateLabel = STATES.find((s) => s.value === tile.state)?.label;

  return (
    <>
      <tr>
        <td>
          <input type="checkbox" checked={checked} onChange={onToggleCheck} />
        </td>
        <td>{formatDateTime(tile.started_at)}</td>
        <td>{tile.trap_name}</td>
        <td className="num">{tile.video_count}</td>
        <td>
          {formatDuration(tile.duration_s)}
          {tile.duration_s >= 3600 && <span className="muted small"> · continue</span>}
        </td>
        <td>
          {tile.species.length > 0 ? (
            <span className="tile__species">
              {tile.species.map((s) => (
                <span key={s.id} className={`chip chip--${s.confidence}`}>
                  <span className="swatch" style={{ background: s.color ?? 'transparent' }} />
                  {s.common_name}
                </span>
              ))}
            </span>
          ) : stateLabel ? (
            <span className="chip">{stateLabel}</span>
          ) : (
            <span className="muted small">—</span>
          )}
        </td>
        <td>
          {!tile.auto_grouped && (
            <span className="badge" title="Découpage manuel : un regroupement ne le défera pas">
              manuel
            </span>
          )}
          {tile.reviewed ? (
            <span className="badge badge--ok">dépouillée</span>
          ) : (
            <span className="muted small">à dépouiller</span>
          )}
        </td>
        <td className="row--actions">
          <button onClick={onOpen}>Dépouiller</button>
          <button onClick={onToggleExpand}>{expanded ? 'Replier' : 'Vidéos'}</button>
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={8}>
            {videos === null ? (
              <p className="muted">Chargement…</p>
            ) : (
              <div className="thumbs" style={{ ['--tile-w' as string]: `${thumbSize}px` }}>
                {videos.map((v, i) => (
                  <figure key={v.id} className="thumb">
                    {v.thumbnail_path ? (
                      <img src={convertFileSrc(v.thumbnail_path)} alt="" loading="lazy" />
                    ) : (
                      <div className="thumb__none">pas de vignette</div>
                    )}
                    <figcaption>
                      <span className="mono small">{v.file_name}</span>
                      <span className="muted small">{formatDateTime(v.recorded_at)}</span>
                      {v.file_state !== 'present' && (
                        <span className="badge badge--warn">
                          {v.file_state === 'missing' ? 'disparue' : 'supprimée'}
                        </span>
                      )}
                      {i > 0 && (
                        <button
                          className="small"
                          onClick={() => onSplit(v.id)}
                          title="Cette vidéo et les suivantes partent dans une nouvelle séquence"
                        >
                          Scinder ici
                        </button>
                      )}
                    </figcaption>
                  </figure>
                ))}
              </div>
            )}
          </td>
        </tr>
      )}
    </>
  );
}
