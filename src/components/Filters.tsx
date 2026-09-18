import { useState } from 'react';

import { CONFIDENCES, SUN_PHASES } from '../api';
import { SpeciesPicker } from './SpeciesPicker';
import type { Confidence as Conf } from '../api';
import type { Confidence, GridFilter, Species, Tag, Trap } from '../api';
import { monthName } from '../format';

export const EMPTY_FILTER: GridFilter = {
  trap_id: null,
  review: 'all',
  states: [],
  species: [],
  tags: [],
  confidence_min: null,
  from: null,
  to: null,
  months: [],
  hour_from: null,
  hour_to: null,
  sun_phases: [],
  duration_min_s: null,
  duration_max_s: null,
  query: null,
  limit: 200,
  offset: 0,
};

/** Combien de critères sont posés — pour dire qu'un filtre est actif sans le déplier. */
export function activeCount(f: GridFilter): number {
  let n = 0;
  if (f.trap_id) n++;
  if (f.review !== 'all') n++;
  n += f.states.length ? 1 : 0;
  n += f.species.length ? 1 : 0;
  n += f.tags.length ? 1 : 0;
  if (f.confidence_min) n++;
  if (f.from || f.to) n++;
  if (f.months.length) n++;
  if (f.hour_from !== null || f.hour_to !== null) n++;
  if (f.sun_phases.length) n++;
  if (f.duration_min_s !== null || f.duration_max_s !== null) n++;
  if (f.query) n++;
  return n;
}

function num(raw: string): number | null {
  const t = raw.trim();
  if (t === '') return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

/** Les raccourcis qui servent vraiment : une saison se choisit d'un clic, pas mois par mois. */
const SEASONS: { label: string; months: number[] }[] = [
  { label: 'Hiver', months: [12, 1, 2] },
  { label: 'Printemps', months: [3, 4, 5] },
  { label: 'Été', months: [6, 7, 8] },
  { label: 'Automne', months: [9, 10, 11] },
];

export function Filters({
  filter,
  onChange,
  traps,
  species,
  tags,
  extra,
  extraCount = 0,
  onClear,
  durationLabel = 'Durée du passage',
  durationHint = 'Une activité continue de deux heures se retrouve ici',
}: {
  filter: GridFilter;
  onChange: (f: GridFilter) => void;
  traps: Trap[];
  species: Species[];
  tags: Tag[];
  /** Critères propres à une vue, ajoutés au panneau déplié. */
  extra?: React.ReactNode;
  /** Combien de ces critères sont posés, pour que le compteur ne mente pas. */
  extraCount?: number;
  /** Remise à zéro complète, critères de la vue compris. */
  onClear?: () => void;
  durationLabel?: string;
  durationHint?: string;
}) {
  const [open, setOpen] = useState(() => {
    // Ouvert par défaut : cachés derrière un bouton, les filtres passaient inaperçus.
    try {
      return localStorage.getItem('filters-open') !== '0';
    } catch {
      return true;
    }
  });

  const setOpenPersisted = (value: boolean) => {
    setOpen(value);
    try {
      localStorage.setItem('filters-open', value ? '1' : '0');
    } catch {
      /* sans conséquence : le choix tient pour la session */
    }
  };
  const set = <K extends keyof GridFilter>(key: K, value: GridFilter[K]) =>
    onChange({ ...filter, [key]: value, offset: 0 });

  const toggle = (key: 'months' | 'species' | 'tags' | 'sun_phases' | 'states', value: never) => {
    const list = filter[key] as unknown[];
    const next = list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
    onChange({ ...filter, [key]: next, offset: 0 });
  };

  const n = activeCount(filter) + extraCount;

  return (
    <section className="panel stack">
      <div className="row row--flush filters">
        <input
          className="search"
          placeholder="Rechercher dans les notes et les noms de fichier…"
          value={filter.query ?? ''}
          onChange={(e) => set('query', e.target.value || null)}
        />

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
          <option value="all">Toutes</option>
          <option value="unreviewed">À dépouiller</option>
          <option value="reviewed">Dépouillées</option>
        </select>

        <label className="inline">
          Du
          <input
            type="date"
            value={filter.from ?? ''}
            onChange={(e) => set('from', e.target.value || null)}
          />
          au
          <input
            type="date"
            value={filter.to ?? ''}
            onChange={(e) => set('to', e.target.value || null)}
          />
        </label>

        <label className="inline">
          Entre
          <input
            type="number"
            min={0}
            max={23}
            className="tiny"
            placeholder="0"
            value={filter.hour_from ?? ''}
            onChange={(e) => set('hour_from', num(e.target.value))}
          />
          h et
          <input
            type="number"
            min={0}
            max={23}
            className="tiny"
            placeholder="23"
            value={filter.hour_to ?? ''}
            onChange={(e) => set('hour_to', num(e.target.value))}
          />
          h
          <span className="muted small" title="22 → 4 retient la nuit entière">
            (22 → 4 traverse minuit)
          </span>
        </label>

        <button
          onClick={() => setOpenPersisted(!open)}
          className={n > 0 ? 'tab--on' : undefined}
        >
          {open ? 'Replier' : 'Filtres'}
          {n > 0 ? ` (${n})` : ''}
        </button>
        {n > 0 && (
          <button onClick={() => (onClear ? onClear() : onChange(EMPTY_FILTER))}>Effacer</button>
        )}
      </div>

      {open && (
        <div className="filters__panel">
          <fieldset>
            <legend>Espèces — l’une OU l’autre</legend>
            <SpeciesPicker
              species={species}
              selected={
                new Map(filter.species.map((id) => [id, 'certain' as Conf]))
              }
              onToggle={(id) => toggle('species', id as never)}
            />
            <label className="inline">
              Confiance minimale
              <select
                value={filter.confidence_min ?? ''}
                onChange={(e) => set('confidence_min', (e.target.value || null) as Confidence | null)}
              >
                <option value="">peu importe</option>
                {CONFIDENCES.map((c) => (
                  <option key={c.value} value={c.value}>
                    {c.label} ou mieux
                  </option>
                ))}
              </select>
            </label>
          </fieldset>

          {tags.length > 0 && (
            <fieldset>
              <legend>Tags — chaque tag ajouté affine (ET)</legend>
              <div className="chips">
                {tags.map((t) => (
                  <label
                    key={t.id}
                    className={filter.tags.includes(t.name) ? 'chip chip--on' : 'chip'}
                  >
                    <input
                      type="checkbox"
                      checked={filter.tags.includes(t.name)}
                      onChange={() => toggle('tags', t.name as never)}
                    />
                    #{t.name} <span className="muted">({t.usage_count})</span>
                  </label>
                ))}
              </div>
            </fieldset>
          )}

          <fieldset>
            <legend>Saison — mois toutes années confondues</legend>
            <p className="muted small">
              Comparer tous les mois de décembre entre eux, et non un hiver donné : c’est le
              cœur de l’analyse d’une année sur l’autre.
            </p>
            <div className="chips">
              {Array.from({ length: 12 }, (_, i) => i + 1).map((m) => (
                <label key={m} className={filter.months.includes(m) ? 'chip chip--on' : 'chip'}>
                  <input
                    type="checkbox"
                    checked={filter.months.includes(m)}
                    onChange={() => toggle('months', m as never)}
                  />
                  {monthName(m)}
                </label>
              ))}
            </div>
            <div className="row row--flush">
              {SEASONS.map((s) => (
                <button key={s.label} className="small" onClick={() => set('months', s.months)}>
                  {s.label}
                </button>
              ))}
              {filter.months.length > 0 && (
                <button className="small" onClick={() => set('months', [])}>
                  Tous les mois
                </button>
              )}
            </div>
          </fieldset>

          <fieldset>
            <legend>Lumière</legend>
            <div className="chips">
              {SUN_PHASES.map((p) => (
                <label
                  key={p.value}
                  className={filter.sun_phases.includes(p.value) ? 'chip chip--on' : 'chip'}
                >
                  <input
                    type="checkbox"
                    checked={filter.sun_phases.includes(p.value)}
                    onChange={() => toggle('sun_phases', p.value as never)}
                  />
                  {p.label}
                </label>
              ))}
            </div>
            <p className="muted small">
              Jour et nuit sont calculés depuis les coordonnées du piège. Sans position, une
              séquence n’est retenue par aucun de ces quatre filtres — on ne devine pas.
            </p>
          </fieldset>

          {extra}

          <fieldset>
            <legend>{durationLabel}</legend>
            <div className="row row--flush">
              <label className="inline">
                Au moins
                <input
                  type="number"
                  min={0}
                  className="tiny"
                  value={filter.duration_min_s != null ? filter.duration_min_s / 60 : ''}
                  onChange={(e) => {
                    const v = num(e.target.value);
                    set('duration_min_s', v === null ? null : v * 60);
                  }}
                />
                min
              </label>
              <label className="inline">
                au plus
                <input
                  type="number"
                  min={0}
                  className="tiny"
                  value={filter.duration_max_s != null ? filter.duration_max_s / 60 : ''}
                  onChange={(e) => {
                    const v = num(e.target.value);
                    set('duration_max_s', v === null ? null : v * 60);
                  }}
                />
                min
              </label>
              <span className="muted small">{durationHint}</span>
            </div>
          </fieldset>
        </div>
      )}
    </section>
  );
}
