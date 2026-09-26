import { useCallback, useEffect, useMemo, useState } from 'react';

import { listSpecies, listTraps, stats as fetchStats } from '../api';
import type { GridFilter, Species, Stats as StatsData, Trap } from '../api';
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

export function Stats({ onError }: { onError: (e: string | null) => void }) {
  const [filter, setFilter] = useState<GridFilter>(EMPTY_FILTER);
  const [data, setData] = useState<StatsData | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [species, setSpecies] = useState<Species[]>([]);
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
      label: hourLabel(h),
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
      <Filters filter={filter} onChange={setFilter} traps={traps} species={species} />

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

          <section className="panel stack">
            <div className="row row--flush">
              <h2>Rythme d’activité</h2>
              <span className="app__spacer" />
              <div className="segmented">
                <button
                  className={axis === 'solar' ? 'segmented__on' : undefined}
                  onClick={() => setAxis('solar')}
                >
                  Heure solaire
                </button>
                <button
                  className={axis === 'civil' ? 'segmented__on' : undefined}
                  onClick={() => setAxis('civil')}
                >
                  Heure civile
                </button>
              </div>
            </div>

            {axis === 'solar' ? (
              <>
                <BarChart bars={solarBars} />
                <p className="muted small">
                  Par tranches de 30 min autour du <strong>coucher du soleil</strong> (☾) de chaque
                  jour.
                  {data.without_position > 0 ? (
                    <>
                      {' '}
                      <span className="warn">
                        {data.without_position} passage(s) sur {data.total_sequences} absent(s)
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
                  {data.without_position > 0 && (
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
            <h2>Rythme par espèce</h2>
            <SmallMultiples
              series={speciesSeries}
              slots={HOURS}
              labelOf={(h) => (h % 6 === 0 ? hourLabel(h) : '')}
              fullLabelOf={hourLabel}
            />
            <Help>
              <p>
                Une ligne par espèce, sur 24 heures, chacune à l’échelle de son propre maximum : on
                compare des <em>formes</em> d’activité, pas des abondances — sinon l’espèce la plus
                fréquente écraserait les autres.
              </p>
            </Help>
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
