import { useCallback, useEffect, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

import { STATES, SUN_PHASES, listSpecies, listTraps, listVideos } from '../api';
import type { Species, Trap, VideoFilter, VideoPage, VideoRow } from '../api';
import { EMPTY_FILTER, Filters } from '../components/Filters';
import { VideoPlayer } from '../components/VideoPlayer';
import { ViewControls, useDisplay, useThumbSize } from '../components/ViewControls';
import { formatDateTime, formatDuration } from '../format';

const EMPTY_VIDEO_FILTER: VideoFilter = {
  ...EMPTY_FILTER,
  file_states: [],
  undated: null,
  unpositioned: null,
  sort: 'date',
};

function humanBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} o`;
  const units = ['Ko', 'Mo', 'Go', 'To'];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
}

/** Toutes les vidéos, une par ligne, filtrables jusqu'au fichier. */
export function Videos({ onError }: { onError: (e: string | null) => void }) {
  const [filter, setFilter] = useState<VideoFilter>(EMPTY_VIDEO_FILTER);
  const [page, setPage] = useState<VideoPage | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [species, setSpecies] = useState<Species[]>([]);
  const [playing, setPlaying] = useState<VideoRow | null>(null);
  // Les fichiers se regardent plutôt en liste : c'est là qu'on cherche une capture
  // précise. La grille sert à balayer d'un coup d'œil.
  const [display, setDisplay] = useDisplay('videos', 'list');
  const [thumbSize, setThumbSize] = useThumbSize('videos', 220);

  const refresh = useCallback(async () => {
    try {
      setPage(await listVideos(filter));
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
  }, []);

  const set = <K extends keyof VideoFilter>(key: K, value: VideoFilter[K]) =>
    setFilter({ ...filter, [key]: value, offset: 0 });

  const rows = page?.rows ?? [];
  const limit = filter.limit;

  return (
    <div className="stack">
      <Filters
        filter={filter}
        onChange={(f) => setFilter({ ...filter, ...f })}
        onClear={() => setFilter(EMPTY_VIDEO_FILTER)}
        extraCount={
          (filter.file_states.length ? 1 : 0) +
          (filter.undated ? 1 : 0) +
          (filter.unpositioned ? 1 : 0)
        }
        durationLabel="Durée de la vidéo"
        durationHint="La durée du fichier, pas celle du passage"
        traps={traps}
        species={species}
        extra={
          <fieldset>
            <legend>État du fichier</legend>
            <div className="chips">
              {[
                { value: 'present', label: 'Lisible' },
                { value: 'purged', label: 'Supprimée' },
                { value: 'missing', label: 'Disparue' },
              ].map((s) => (
                <label
                  key={s.value}
                  className={filter.file_states.includes(s.value) ? 'chip chip--on' : 'chip'}
                >
                  <input
                    type="checkbox"
                    checked={filter.file_states.includes(s.value)}
                    onChange={(e) =>
                      set(
                        'file_states',
                        e.target.checked
                          ? [...filter.file_states, s.value]
                          : filter.file_states.filter((v) => v !== s.value),
                      )
                    }
                  />
                  {s.label}
                </label>
              ))}
            </div>
            <div className="chips">
              <label className={filter.undated ? 'chip chip--on' : 'chip'}>
                <input
                  type="checkbox"
                  checked={!!filter.undated}
                  onChange={(e) => set('undated', e.target.checked ? true : null)}
                />
                Sans date
              </label>
              <label className={filter.unpositioned ? 'chip chip--on' : 'chip'}>
                <input
                  type="checkbox"
                  checked={!!filter.unpositioned}
                  onChange={(e) => set('unpositioned', e.target.checked ? true : null)}
                />
                Sans position
              </label>
            </div>
            <p className="muted small">
              Ici les filtres de date et d’heure portent sur <strong>la vidéo</strong>, pas sur le
              début de son passage : chercher « entre 2 h et 3 h » rend les déclenchements de
              cette tranche.
            </p>
          </fieldset>
        }
      />

      <div className="row row--flush">
        <span className="muted small">
          {page ? `${page.total} vidéo(s)` : '…'}
          {page && page.total > 0 && (
            <>
              {' · '}
              {formatDuration(Math.round(page.total_duration_s))} · {humanBytes(page.total_bytes)}
            </>
          )}
          {page && page.undated_total > 0 && ` · ${page.undated_total} sans date au total`}
          {page && page.unplayable_total > 0 && ` · ${page.unplayable_total} illisible(s)`}
        </span>
        <span className="app__spacer" />
        <select value={filter.sort} onChange={(e) => set('sort', e.target.value)}>
          <option value="date">Plus récentes d’abord</option>
          <option value="date_asc">Plus anciennes d’abord</option>
        </select>
        <ViewControls
          display={display}
          onDisplay={setDisplay}
          size={thumbSize}
          onSize={setThumbSize}
          tilesLabel="Grille"
        />
        <button onClick={() => setFilter(EMPTY_VIDEO_FILTER)}>Réinitialiser</button>
      </div>

      <section className="panel stack">
        {rows.length === 0 ? (
          <p className="muted">Aucune vidéo ne correspond à ces filtres.</p>
        ) : display === 'tiles' ? (
          <div className="tiles" style={{ ['--tile-w' as string]: `${thumbSize}px` }}>
            {rows.map((v) => (
              <button
                key={v.id}
                type="button"
                className="tile"
                onClick={() => v.file_state === 'present' && setPlaying(v)}
                title={v.file_state === 'present' ? 'Lire' : 'Le fichier n’est plus là'}
              >
                <div className="tile__image">
                  {v.thumbnail_path ? (
                    <img src={convertFileSrc(v.thumbnail_path)} alt="" loading="lazy" />
                  ) : (
                    <div className="tile__none">pas de vignette</div>
                  )}
                  {v.duration_s != null && (
                    <span className="tile__count">{v.duration_s.toFixed(0)} s</span>
                  )}
                  {v.file_state !== 'present' && (
                    <span className="tile__warn" title="Fichier supprimé ou disparu">
                      ⌀
                    </span>
                  )}
                </div>
                <div className="tile__meta">
                  <span className="small">
                    {v.recorded_at ? (
                      formatDateTime(v.recorded_at)
                    ) : (
                      <span className="danger">sans date</span>
                    )}
                  </span>
                  <span className="muted small">
                    {v.trap_name}
                    {v.sun_phase &&
                      ` · ${SUN_PHASES.find((p) => p.value === v.sun_phase)?.label.toLowerCase()}`}
                  </span>
                  {v.species.length > 0 && (
                    <span className="tile__species">
                      {v.species.map((name, i) => (
                        <span key={name} className="chip">
                          <span
                            className="swatch"
                            style={{ background: v.species_colors[i] ?? 'transparent' }}
                          />
                          {name}
                        </span>
                      ))}
                    </span>
                  )}
                </div>
              </button>
            ))}
          </div>
        ) : (
          <table className="table table--videos">
            <thead>
              <tr>
                <th />
                <th>Date</th>
                <th>Piège</th>
                <th>Espèces</th>
                <th>Durée</th>
                <th>Fichier</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((v) => (
                <tr key={v.id} className={v.file_state === 'present' ? undefined : 'inactive'}>
                  <td>
                    {v.thumbnail_path ? (
                      <img
                        className="thumb--mini"
                        style={{ width: thumbSize / 2 }}
                        src={convertFileSrc(v.thumbnail_path)}
                        alt=""
                        loading="lazy"
                        onClick={() => v.file_state === 'present' && setPlaying(v)}
                      />
                    ) : (
                      <span className="thumb--mini thumb__none" style={{ width: thumbSize / 2 }}>
                        —
                      </span>
                    )}
                  </td>
                  <td>
                    {v.recorded_at ? (
                      <>
                        {formatDateTime(v.recorded_at)}
                        {v.date_manual && (
                          <span className="badge" title="Date saisie à la main">
                            corrigée
                          </span>
                        )}
                      </>
                    ) : (
                      <span className="danger">sans date</span>
                    )}
                    {v.sun_phase && (
                      <div className="muted small">
                        {SUN_PHASES.find((p) => p.value === v.sun_phase)?.label.toLowerCase()}
                      </div>
                    )}
                  </td>
                  <td>
                    {v.trap_name}
                    {v.sequence_size > 1 && (
                      <div className="muted small">passage de {v.sequence_size} vidéos</div>
                    )}
                  </td>
                  <td>
                    {v.species.length > 0 ? (
                      <span className="tile__species">
                        {v.species.map((name, i) => (
                          <span key={name} className="chip">
                            <span
                              className="swatch"
                              style={{ background: v.species_colors[i] ?? 'transparent' }}
                            />
                            {name}
                          </span>
                        ))}
                      </span>
                    ) : v.state ? (
                      <span className="chip">
                        {STATES.find((s) => s.value === v.state)?.label ?? v.state}
                      </span>
                    ) : (
                      <span className="muted small">à dépouiller</span>
                    )}
                  </td>
                  <td className="num muted">
                    {v.duration_s != null ? `${v.duration_s.toFixed(0)} s` : '—'}
                  </td>
                  <td className="mono small muted">
                    {v.file_name}
                    {v.file_state !== 'present' && (
                      <span className="badge badge--warn">
                        {v.file_state === 'missing' ? 'disparue' : 'supprimée'}
                      </span>
                    )}
                    {v.latitude == null && (
                      <span className="badge badge--warn">sans position</span>
                    )}
                  </td>
                  <td className="row--actions">
                    <button
                      onClick={() => setPlaying(v)}
                      disabled={v.file_state !== 'present'}
                      title={
                        v.file_state !== 'present'
                          ? 'Le fichier n’est plus là : la donnée reste, pas l’image'
                          : undefined
                      }
                    >
                      Lire
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {page && page.total > limit && (
          <div className="row row--flush">
            <button
              onClick={() => setFilter({ ...filter, offset: Math.max(0, filter.offset - limit) })}
              disabled={filter.offset === 0}
            >
              Précédentes
            </button>
            <span className="muted small">
              {filter.offset + 1}–{Math.min(filter.offset + limit, page.total)} sur {page.total}
            </span>
            <button
              onClick={() => setFilter({ ...filter, offset: filter.offset + limit })}
              disabled={filter.offset + limit >= page.total}
            >
              Suivantes
            </button>
          </div>
        )}
      </section>

      {playing && (
        <VideoPlayer
          filePath={playing.file_path}
          title={`${playing.trap_name} — ${formatDateTime(playing.recorded_at)}`}
          subtitle={playing.file_name}
          onClose={() => setPlaying(null)}
        />
      )}
    </div>
  );
}
