import { useCallback, useEffect, useMemo, useState } from 'react';

import { listSpecies, listTags, listTraps, stats as fetchStats } from '../api';
import type { GridFilter, Species, Stats as StatsData, Tag, Trap } from '../api';
import { BarChart, SmallMultiples } from '../components/Charts';
import { Export } from '../components/Export';
import { EMPTY_FILTER, Filters } from '../components/Filters';
import { formatDate, monthName } from '../format';

const HOURS = Array.from({ length: 24 }, (_, i) => i);

/** Tranches de 30 min, de 4 h avant le coucher à 10 h après. */
const SOLAR_SLOTS = Array.from({ length: 29 }, (_, i) => -240 + i * 30);

function solarLabel(minutes: number): string {
  if (minutes === 0) return '☾';
  const h = minutes / 60;
  return Number.isInteger(h) ? `${h > 0 ? '+' : ''}${h}` : '';
}

export function Stats({ onError }: { onError: (e: string | null) => void }) {
  const [filter, setFilter] = useState<GridFilter>(EMPTY_FILTER);
  const [data, setData] = useState<StatsData | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [species, setSpecies] = useState<Species[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [axis, setAxis] = useState<'civil' | 'solar'>('solar');

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
    listTags().then(setTags).catch(() => undefined);
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
    return [...map.values()];
  }, [data]);

  const solarBars = useMemo(() => {
    if (!data) return [];
    const byBucket = new Map(data.solar.map((s) => [s.bucket, s.count]));
    return SOLAR_SLOTS.map((slot) => ({
      label: solarLabel(slot),
      value: byBucket.get(slot) ?? 0,
      dim: slot < 0,
    }));
  }, [data]);

  const hourBars = useMemo(() => {
    if (!data) return [];
    const byHour = new Map(data.hours.map((h) => [h.hour, h.count]));
    return HOURS.map((h) => ({
      label: String(h).padStart(2, '0'),
      value: byHour.get(h) ?? 0,
    }));
  }, [data]);

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
      <Filters filter={filter} onChange={setFilter} traps={traps} species={species} tags={tags} />

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
          <section className="panel stack">
            <h2>Vue d’ensemble</h2>
            <ul className="tallies">
              <li>
                <span className="tallies__n">{data.total_sequences}</span> passages
              </li>
              <li>
                <span className="tallies__n">{data.total_videos}</span> déclenchements
              </li>
              <li>
                <span className="tallies__n">{data.identified_sequences}</span> avec une espèce
              </li>
              <li className={data.unreviewed_sequences ? undefined : 'muted'}>
                <span className="tallies__n">{data.unreviewed_sequences}</span> à dépouiller
              </li>
            </ul>
          </section>

          <section className="panel stack">
            <div className="row row--flush">
              <h2>Rythme d’activité</h2>
              <span className="app__spacer" />
              <button
                className={axis === 'solar' ? 'tab--on' : undefined}
                onClick={() => setAxis('solar')}
              >
                Heure solaire
              </button>
              <button
                className={axis === 'civil' ? 'tab--on' : undefined}
                onClick={() => setAxis('civil')}
              >
                Heure civile
              </button>
            </div>

            {axis === 'solar' ? (
              <>
                <p className="muted small">
                  Heures avant et après le coucher du soleil (☾ = coucher). C’est la lecture qui
                  a du sens pour un animal : un renard sort « au crépuscule », pas « à 18 h ».
                  En heure civile, la même habitude semble se déplacer de plusieurs heures entre
                  juin et décembre.
                </p>
                <BarChart bars={solarBars} />
                {data.without_position > 0 && (
                  <p className="muted small">
                    {data.without_position} passage(s) absent(s) de ce graphique : leur piège n’a
                    pas de coordonnées, donc ni lever ni coucher calculables. Renseigne sa position
                    dans l’onglet Pièges.
                  </p>
                )}
              </>
            ) : (
              <>
                <p className="muted small">
                  Heure de l’horloge de la caméra, corrigée de son décalage.
                </p>
                <BarChart bars={hourBars} />
              </>
            )}
          </section>

          <section className="panel stack">
            <h2>Rythme par espèce</h2>
            <p className="muted small">
              Une ligne par espèce, sur l’axe des 24 heures. Chaque ligne est mise à l’échelle de
              son propre maximum : on compare des <em>formes</em> d’activité, pas des abondances —
              sinon l’espèce la plus fréquente écraserait toutes les autres.
            </p>
            <SmallMultiples
              series={speciesSeries}
              slots={HOURS}
              labelOf={(h) => (h % 6 === 0 ? String(h).padStart(2, '0') : '')}
            />
          </section>

          <section className="panel stack">
            <h2>Saisonnalité</h2>
            <p className="muted small">
              Tous les mois de janvier ensemble, tous les février ensemble, quelle que soit
              l’année. Le nombre d’années qui nourrissent chaque mois est indiqué dessous.
            </p>
            <BarChart bars={monthBars} />
          </section>

          <section className="panel stack">
            <h2>Comparaison entre emplacements</h2>
            <div className="notice notice--warn panel">
              <strong>À lire avec prudence.</strong> Ces chiffres sont des comptages bruts. Sans
              effort de piégeage enregistré, deux pièges ne sont pas comparables si l’un a tourné
              trois mois et l’autre trois semaines. La colonne « période » donne l’écart entre la
              première et la dernière capture — ce n’est <em>pas</em> une durée de fonctionnement :
              les batteries vides et les pièges relevés n’y apparaissent pas.
            </div>
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
          </section>

          <Export filter={filter} selection={[]} total={data.total_sequences} onError={onError} />

          <section className="panel stack">
            <h2>Par espèce</h2>
            {data.species.length === 0 ? (
              <p className="muted">Aucune espèce identifiée dans cette sélection.</p>
            ) : (
              <table className="table">
                <thead>
                  <tr>
                    <th />
                    <th>Espèce</th>
                    <th className="num">Passages</th>
                    <th className="num">Déclenchements</th>
                    <th className="num">dont certains</th>
                    <th className="num">Pièges</th>
                    <th>Première</th>
                    <th>Dernière</th>
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
                        />
                      </td>
                      <td>{s.common_name}</td>
                      <td className="num">{s.sequences}</td>
                      <td className="num">{s.videos}</td>
                      <td className="num muted">{s.certain}</td>
                      <td className="num">{s.traps}</td>
                      <td className="muted small">{formatDate(s.first_at)}</td>
                      <td className="muted small">{formatDate(s.last_at)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
        </>
      )}
    </div>
  );
}
