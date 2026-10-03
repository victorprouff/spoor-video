import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { listSpecies, listTraps, recentActivity, stats as fetchStats } from '../api';
import type {
  GridFilter,
  RecentBucket,
  RhythmBucket,
  Species,
  Stats as StatsData,
  Trap,
} from '../api';
import { BarChart, SmallMultiples } from '../components/Charts';
import { Export } from '../components/Export';
import { EMPTY_FILTER, Filters } from '../components/Filters';
import { formatDate, monthName } from '../format';

const HOURS = Array.from({ length: 24 }, (_, i) => i);

/** Une heure se lit « 06 h », jamais « 06 » : le nombre seul peut être n'importe quoi. */
const hourLabel = (h: number) => `${String(h).padStart(2, '0')} h`;

/**
 * Tranches de 30 minutes sur un cycle complet, de 12 h avant le coucher à 12 h après.
 *
 * La fenêtre couvre volontairement les 24 heures : une version plus étroite (−4 h → +10 h)
 * faisait **disparaître sans rien dire** les passages de plein après-midi, à six ou huit
 * heures du coucher. Un graphique qui omet des données en silence est pire qu'un
 * graphique large.
 */
const SOLAR_SLOTS = Array.from({ length: 49 }, (_, i) => -720 + i * 30);

function solarLabel(minutes: number): string {
  if (minutes === 0) return '☾';
  const h = minutes / 60;
  // Une graduation toutes les deux heures : sur 49 barres, tout étiqueter est illisible.
  return Number.isInteger(h) && h % 2 === 0 ? `${h > 0 ? '+' : ''}${h}` : '';
}

/** Clé de l'option « Sans espèce » : un passage non identifié a sa place dans le rythme. */
const NO_SPECIES = '';

type RhythmView = 'species' | 'solar' | 'civil';

export function Stats({ onError }: { onError: (e: string | null) => void }) {
  const [filter, setFilter] = useState<GridFilter>(EMPTY_FILTER);
  const [data, setData] = useState<StatsData | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [species, setSpecies] = useState<Species[]>([]);
  const [view, setView] = useState<RhythmView>('species');
  // On retient les espèces **écartées**, pas les retenues : une espèce qui apparaît en
  // changeant de filtre est ainsi cochée d'office, comme le veut « toutes par défaut ».
  const [hidden, setHidden] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    try {
      setData(await fetchStats(filter));
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

  const speciesSeries = useMemo(() => {
    if (!data) return [];
    const map = new Map<
      string,
      { key: string; name: string; color: string | null; values: Map<number, number> }
    >();
    for (const row of data.species_hours) {
      let entry = map.get(row.species_id);
      if (!entry) {
        entry = {
          key: row.species_id,
          name: row.common_name,
          color: row.color,
          values: new Map(),
        };
        map.set(row.species_id, entry);
      }
      entry.values.set(row.hour, row.count);
    }
    // Des plus fréquentes aux plus rares : l'œil commence par ce qui compte le plus.
    const total = (v: Map<number, number>) => [...v.values()].reduce((a, b) => a + b, 0);
    return [...map.values()].sort(
      (a, b) => total(b.values) - total(a.values) || a.name.localeCompare(b.name, 'fr'),
    );
  }, [data]);

  /** Les choix de la liste déroulante, dans l'ordre du tableau « Par espèce ». */
  const speciesOptions = useMemo(() => {
    if (!data) return [];
    const options = data.species.map((s) => ({
      key: s.species_id,
      name: s.common_name,
      color: s.color,
      count: s.sequences,
    }));
    const unidentified = data.total_sequences - data.identified_sequences;
    if (unidentified > 0) {
      options.push({ key: NO_SPECIES, name: 'Sans espèce', color: null, count: unidentified });
    }
    return options;
  }, [data]);

  /** Passages retenus par la liste déroulante — chacun une seule fois. */
  const rhythm = useMemo(() => {
    if (!data) return [];
    const kept = (r: RhythmBucket) =>
      r.species_ids.length === 0
        ? !hidden.has(NO_SPECIES)
        : r.species_ids.some((id) => !hidden.has(id));
    return data.rhythm.filter(kept);
  }, [data, hidden]);

  const keptCount = speciesOptions.filter((o) => !hidden.has(o.key)).length;
  const withoutPosition = rhythm
    .filter((r) => r.solar_bucket === null)
    .reduce((sum, r) => sum + r.count, 0);
  const rhythmTotal = rhythm.reduce((sum, r) => sum + r.count, 0);

  const solarBars = useMemo(() => {
    const byBucket = new Map<number, number>();
    for (const r of rhythm) {
      if (r.solar_bucket === null) continue;
      byBucket.set(r.solar_bucket, (byBucket.get(r.solar_bucket) ?? 0) + r.count);
    }
    return SOLAR_SLOTS.map((slot) => ({
      label: solarLabel(slot),
      value: byBucket.get(slot) ?? 0,
      dim: slot < 0,
    }));
  }, [rhythm]);

  const hourBars = useMemo(() => {
    const byHour = new Map<number, number>();
    for (const r of rhythm) byHour.set(r.hour, (byHour.get(r.hour) ?? 0) + r.count);
    return HOURS.map((h) => ({
      label: hourLabel(h),
      value: byHour.get(h) ?? 0,
    }));
  }, [rhythm]);

  const monthBars = useMemo(() => {
    if (!data) return [];
    const byMonth = new Map(data.months.map((m) => [m.month, m]));
    return Array.from({ length: 12 }, (_, i) => i + 1).map((m) => {
      const entry = byMonth.get(m);
      return {
        label: monthName(m).slice(0, 4),
        value: entry?.count ?? 0,
        // Le nombre d'années qui nourrissent ce mois : sans lui, un mois très couvert
        // paraît simplement plus fréquenté.
        hint: entry && entry.years > 0 ? `${entry.years} an${entry.years > 1 ? 's' : ''}` : '',
      };
    });
  }, [data]);

  return (
    <div className="stack">
      <Filters
        filter={filter}
        onChange={setFilter}
        traps={traps}
        species={species}
        showReview={false}
      />

      {!data ? (
        <section className="panel">
          <p className="muted">Calcul…</p>
        </section>
      ) : data.total_sequences === 0 ? (
        <section className="panel">
          <p className="muted">Aucune séquence dans cette sélection.</p>
        </section>
      ) : (
        <>
          <section className="panel kpis">
            <div className="kpi">
              <span className="kpi__n">{data.total_sequences}</span>
              <span className="kpi__label">passages</span>
            </div>
            <div className="kpi">
              <span className="kpi__n">{data.total_videos}</span>
              <span className="kpi__label">déclenchements</span>
            </div>
            <div className="kpi">
              <span className="kpi__n">{data.identified_sequences}</span>
              <span className="kpi__label">avec une espèce</span>
            </div>
            <div className="kpi">
              <span className="kpi__n">{data.species.length}</span>
              <span className="kpi__label">espèce{data.species.length > 1 ? 's' : ''}</span>
            </div>
            {data.unreviewed_sequences > 0 && (
              <div className="kpi kpi--quiet">
                <span className="kpi__n">{data.unreviewed_sequences}</span>
                <span className="kpi__label">à dépouiller</span>
              </div>
            )}
          </section>

          <RecentActivity filter={filter} species={species} onError={onError} />

          <section className="panel stack">
            <div className="row row--flush">
              <h2>Rythme d’activité</h2>
              <span className="app__spacer" />
              {view !== 'species' && (
                <SpeciesDropdown options={speciesOptions} hidden={hidden} onChange={setHidden} />
              )}
              <div className="segmented">
                <button
                  className={view === 'species' ? 'segmented__on' : undefined}
                  onClick={() => setView('species')}
                >
                  Espèces
                </button>
                <button
                  className={view === 'solar' ? 'segmented__on' : undefined}
                  onClick={() => setView('solar')}
                >
                  Heure solaire
                </button>
                <button
                  className={view === 'civil' ? 'segmented__on' : undefined}
                  onClick={() => setView('civil')}
                >
                  Heure civile
                </button>
              </div>
            </div>

            {view === 'species' ? (
              <>
                <SmallMultiples
                  series={speciesSeries}
                  slots={HOURS}
                  labelOf={(h) => (h % 6 === 0 ? hourLabel(h) : '')}
                  fullLabelOf={hourLabel}
                />
                {data.identified_sequences < data.total_sequences && (
                  <p className="muted small">
                    {data.total_sequences - data.identified_sequences} passage(s) sans espèce
                    identifiée n’apparaissent pas ici ; ils sont dans les vues en heure solaire et
                    civile.
                  </p>
                )}
                <Help>
                  <p>
                    Une ligne par espèce, des plus fréquentes aux plus rares, sur 24 heures en
                    heure civile. Chacune est à l’échelle de son propre maximum : on compare des{' '}
                    <em>formes</em> d’activité, pas des abondances — sinon l’espèce la plus
                    fréquente écraserait les autres.
                  </p>
                </Help>
              </>
            ) : keptCount === 0 ? (
              <p className="muted">Aucune espèce choisie.</p>
            ) : view === 'solar' ? (
              <>
                <BarChart bars={solarBars} />
                <p className="muted small">
                  Par tranches de 30 min autour du <strong>coucher du soleil</strong> (☾) de chaque
                  jour.
                  {withoutPosition > 0 ? (
                    <>
                      {' '}
                      <span className="warn">
                        {withoutPosition} passage(s) sur {rhythmTotal} absent(s)
                      </span>{' '}
                      : leur piège n’a pas de coordonnées (Réglages → Pièges).
                    </>
                  ) : null}
                </p>
                <Help>
                  <p>
                    <strong>−2</strong> = deux heures avant le coucher, <strong>+3</strong> = trois
                    heures après ; les barres avant le coucher sont atténuées.
                  </p>
                  <p>
                    C’est la lecture qui a du sens pour un animal. Un renard qui sort une heure
                    après le crépuscule passe à 21 h 50 en juin et à 17 h 50 en décembre : en heure
                    civile il semble avoir deux habitudes, ici il n’en a qu’une. Une espèce diurne,
                    au contraire, s’y étale : pour elle, l’heure civile se lit mieux.
                  </p>
                  <p>
                    Un passage où figurent plusieurs espèces choisies ne compte qu’une fois.
                  </p>
                  {withoutPosition > 0 && (
                    <p>
                      Sans coordonnées, ni lever ni coucher ne se calculent. Renseigne la position
                      du piège, puis « Appliquer aux vidéos ».
                    </p>
                  )}
                </Help>
              </>
            ) : (
              <>
                <BarChart bars={hourBars} />
                <p className="muted small">
                  Heure de l’horloge de la caméra, corrigée de son décalage.
                </p>
              </>
            )}
          </section>

          <section className="panel stack">
            <h2>Saisonnalité</h2>
            <BarChart bars={monthBars} />
            <Help>
              <p>
                Tous les mois de janvier ensemble, tous les février ensemble, quelle que soit
                l’année. Le nombre d’années qui nourrissent chaque mois est indiqué dessous.
              </p>
            </Help>
          </section>

          <section className="panel stack">
            <h2>Par espèce</h2>
            {data.species.length === 0 ? (
              <p className="muted">Aucune espèce identifiée dans cette sélection.</p>
            ) : (
              <table className="table">
                <thead>
                  <tr>
                    <th>Espèce</th>
                    <th className="num">Passages</th>
                    <th className="num">Déclenchements</th>
                    <th className="num">dont certains</th>
                    <th className="num">Pièges</th>
                    <th>Période</th>
                  </tr>
                </thead>
                <tbody>
                  {data.species.map((s) => (
                    <tr key={s.species_id}>
                      <td>
                        <span
                          className="swatch"
                          style={{ background: s.color ?? 'transparent' }}
                          aria-hidden
                        />{' '}
                        {s.common_name}
                      </td>
                      <td className="num">{s.sequences}</td>
                      <td className="num">{s.videos}</td>
                      <td className="num muted">{s.certain}</td>
                      <td className="num">{s.traps}</td>
                      <td className="muted small">
                        {formatDate(s.first_at)} → {formatDate(s.last_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>

          <section className="panel stack">
            <h2>Par piège</h2>
            <table className="table">
              <thead>
                <tr>
                  <th>Piège</th>
                  <th className="num">Passages</th>
                  <th className="num">Déclenchements</th>
                  <th className="num">Espèces</th>
                  <th>Période</th>
                  <th className="num">Étendue</th>
                </tr>
              </thead>
              <tbody>
                {data.traps.map((t) => (
                  <tr key={t.trap_id}>
                    <td>{t.name}</td>
                    <td className="num">{t.sequences}</td>
                    <td className="num">{t.videos}</td>
                    <td className="num">{t.species_richness}</td>
                    <td className="muted small">
                      {formatDate(t.first_at)} → {formatDate(t.last_at)}
                    </td>
                    <td className="num muted">{t.span_days != null ? `${t.span_days} j` : '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="muted small">
              Comptages bruts : deux pièges ne se comparent pas s’ils n’ont pas tourné aussi
              longtemps.
            </p>
            <Help label="Pourquoi ?">
              <p>
                Sans effort de piégeage enregistré, un piège qui a tourné trois mois et un autre
                trois semaines ne sont pas comparables. La « période » va de la première à la
                dernière capture : ce n’est <em>pas</em> une durée de fonctionnement — batteries
                vides et pièges relevés n’y apparaissent pas.
              </p>
            </Help>
          </section>

          <Export
            filter={filter}
            selection={[]}
            total={data.total_sequences}
            onError={onError}
            compact
          />
        </>
      )}
    </div>
  );
}

/** Le mode d'emploi d'un graphique : utile une fois, encombrant ensuite. Replié par défaut. */
function Help({
  children,
  label = 'Comment lire ce graphique',
}: {
  children: React.ReactNode;
  label?: string;
}) {
  return (
    <details className="help">
      <summary>{label}</summary>
      <div className="help__body">{children}</div>
    </details>
  );
}

type SpeciesOption = { key: string; name: string; color: string | null; count: number };

/**
 * Choix des espèces du rythme d'activité : une liste à cocher dans un menu déroulant,
 * pour ne pas pousser le graphique sous une rangée de puces.
 */
function SpeciesDropdown({
  options,
  hidden,
  onChange,
}: {
  options: SpeciesOption[];
  hidden: Set<string>;
  onChange: (hidden: Set<string>) => void;
}) {
  const keptCount = options.filter((o) => !hidden.has(o.key)).length;
  const ref = useRef<HTMLDetailsElement>(null);

  // Un clic ailleurs referme le menu, comme n'importe quelle liste déroulante.
  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (ref.current?.open && !ref.current.contains(e.target as Node)) {
        ref.current.open = false;
      }
    };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, []);

  const toggle = (key: string) => {
    const next = new Set(hidden);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    onChange(next);
  };

  const summary =
    keptCount === options.length
      ? 'Toutes les espèces'
      : keptCount === 0
        ? 'Aucune espèce'
        : keptCount === 1
          ? options.find((o) => !hidden.has(o.key))?.name
          : `${keptCount} espèces sur ${options.length}`;

  return (
    <details className="dropdown" ref={ref}>
      <summary>{summary}</summary>
      <div className="dropdown__menu">
        <div className="dropdown__actions">
          <button className="small" onClick={() => onChange(new Set())}>
            Toutes
          </button>
          <button
            className="small"
            onClick={() => onChange(new Set(options.map((o) => o.key)))}
          >
            Aucune
          </button>
        </div>
        {options.map((o) => (
          <label key={o.key || 'none'} className="dropdown__item">
            <input type="checkbox" checked={!hidden.has(o.key)} onChange={() => toggle(o.key)} />
            <span
              className="swatch"
              style={{ background: o.color ?? 'transparent' }}
              aria-hidden
            />
            <span className="dropdown__name">{o.name}</span>
            <span className="muted small">{o.count}</span>
          </label>
        ))}
      </div>
    </details>
  );
}

const PERIODS = [7, 15, 30, 60, 90];

/**
 * Au-delà, une barre par jour. Heure par heure, 90 jours feraient 2 160 barres de moins
 * d'un pixel : on ne lirait plus rien.
 */
const HOURLY_MAX_DAYS = 15;

const WEEKDAYS = ['dim.', 'lun.', 'mar.', 'mer.', 'jeu.', 'ven.', 'sam.'];
const SHORT_MONTHS = [
  'janv.', 'févr.', 'mars', 'avr.', 'mai', 'juin',
  'juil.', 'août', 'sept.', 'oct.', 'nov.', 'déc.',
];

/** « 14 sept. » : assez court pour une graduation par semaine sur 90 jours. */
function shortDayLabel(day: string): string {
  return `${Number(day.slice(8, 10))} ${SHORT_MONTHS[Number(day.slice(5, 7)) - 1]}`;
}

/** `YYYY-MM-DD` d'une date du calendrier — sans fuseau : seuls jour, mois, année comptent. */
const isoDay = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

/** « lun. 28 » à partir de `YYYY-MM-DD`, en découpant la chaîne (voir format.ts). */
function dayLabel(day: string): string {
  return `${WEEKDAYS[weekday(day)]} ${Number(day.slice(8, 10))}`;
}

/** « 27 septembre » : la période doit se lire même à cheval sur deux mois. */
function fullDayLabel(day: string): string {
  const [, m, d] = day.split('-').map(Number);
  return `${d} ${monthName(m)}`;
}

/** Jour de la semaine de `YYYY-MM-DD`, 0 = dimanche. */
function weekday(day: string): number {
  const [y, m, d] = day.split('-').map(Number);
  return new Date(Date.UTC(y, m - 1, d)).getUTCDay();
}

/**
 * Les 7 à 90 jours qui finissent **aujourd'hui** (date de l'ordinateur) : heure par heure
 * jusqu'à 15 jours, jour par jour au-delà.
 *
 * Les filtres de l'onglet s'appliquent, sauf les dates : la période est celle-ci et pas
 * une autre. Comme les cartes ne sont relevées que de temps en temps, la période peut
 * être vide ; on le dit plutôt que de reculer en silence jusqu'à la dernière capture.
 */
function RecentActivity({
  filter,
  species,
  onError,
}: {
  filter: GridFilter;
  species: Species[];
  onError: (e: string | null) => void;
}) {
  const [rows, setRows] = useState<RecentBucket[] | null>(null);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [period, setPeriod] = useState(7);
  const hourly = period <= HOURLY_MAX_DAYS;

  // Recalculé à chaque requête : l'application peut rester ouverte d'un jour à l'autre.
  const days = useMemo(() => {
    const today = new Date();
    return Array.from({ length: period }, (_, i) =>
      isoDay(new Date(today.getFullYear(), today.getMonth(), today.getDate() - (period - 1 - i))),
    );
  }, [filter, period]);

  useEffect(() => {
    let live = true;
    recentActivity({ ...filter, from: days[0], to: days[days.length - 1] })
      .then((r) => live && setRows(r))
      .catch((e) => onError(String(e)));
    return () => {
      live = false;
    };
  }, [filter, days, onError]);

  const options = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of rows ?? []) {
      const keys = r.species_ids.length === 0 ? [NO_SPECIES] : r.species_ids;
      for (const k of keys) counts.set(k, (counts.get(k) ?? 0) + r.count);
    }
    const byId = new Map(species.map((s) => [s.id, s]));
    return [...counts.entries()]
      .map(([key, count]) => ({
        key,
        name: key === NO_SPECIES ? 'Sans espèce' : (byId.get(key)?.common_name ?? key),
        color: key === NO_SPECIES ? null : (byId.get(key)?.color ?? null),
        count,
      }))
      .sort(
        (a, b) =>
          // « Sans espèce » toujours en dernier, comme dans le rythme d'activité.
          Number(a.key === NO_SPECIES) - Number(b.key === NO_SPECIES) ||
          b.count - a.count ||
          a.name.localeCompare(b.name, 'fr'),
      );
  }, [rows, species]);

  const bars = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of rows ?? []) {
      const kept =
        r.species_ids.length === 0
          ? !hidden.has(NO_SPECIES)
          : r.species_ids.some((id) => !hidden.has(id));
      if (!kept) continue;
      const k = hourly ? `${r.day} ${r.hour}` : r.day;
      counts.set(k, (counts.get(k) ?? 0) + r.count);
    }
    if (!hourly) {
      return days.map((day, i) => ({
        // Une étiquette par lundi : une par jour se chevaucherait dès 30 jours. Rien
        // contre les bords, où l'étiquette centrée déborderait du graphique.
        label:
          weekday(day) === 1 && i >= 2 && i < days.length - 2 ? shortDayLabel(day) : '',
        // Le mois dans la bulle : sur 90 jours, « sam. 3 » ne dit pas lequel.
        title: `${WEEKDAYS[weekday(day)]} ${shortDayLabel(day)}`,
        value: counts.get(day) ?? 0,
      }));
    }
    return days.flatMap((day, i) =>
      HOURS.map((h) => ({
        // Le nom du jour sous midi : centré sous sa journée, il ne gêne pas ses voisins.
        label: h === 12 ? dayLabel(day) : '',
        title: `${dayLabel(day)}, ${hourLabel(h)}`,
        value: counts.get(`${day} ${h}`) ?? 0,
        // Une journée sur deux atténuée : sans cela, les barres ne se découpent pas en jours.
        dim: i % 2 === 1,
      })),
    );
  }, [rows, hidden, days, hourly]);

  const total = bars.reduce((sum, b) => sum + b.value, 0);
  const kept = options.filter((o) => !hidden.has(o.key)).length;

  return (
    <section className="panel stack">
      <div className="row row--flush">
        <h2>Derniers jours</h2>
        <span className="muted small">
          du {fullDayLabel(days[0])} au {fullDayLabel(days[days.length - 1])}
        </span>
        <span className="app__spacer" />
        {options.length > 0 && (
          <SpeciesDropdown options={options} hidden={hidden} onChange={setHidden} />
        )}
        <div className="segmented">
          {PERIODS.map((p) => (
            <button
              key={p}
              className={period === p ? 'segmented__on' : undefined}
              onClick={() => setPeriod(p)}
            >
              {p} j
            </button>
          ))}
        </div>
      </div>

      {rows === null ? (
        <p className="muted">Calcul…</p>
      ) : rows.length === 0 ? (
        <p className="muted">
          Aucun passage sur ces {period} jours. Les vidéos les plus récentes ne sont peut-être pas
          encore importées.
        </p>
      ) : kept === 0 ? (
        <p className="muted">Aucune espèce choisie.</p>
      ) : (
        <>
          <BarChart bars={bars} unit="passage" />
          <p className="muted small">
            {total} passage{total > 1 ? 's' : ''},{' '}
            {hourly ? 'heure par heure' : 'jour par jour'}, en heure de l’horloge de la caméra.
          </p>
        </>
      )}
      {(filter.from || filter.to) && (
        <p className="muted small">Les dates choisies dans les filtres ne s’appliquent pas ici.</p>
      )}
    </section>
  );
}
