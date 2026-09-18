import { useCallback, useEffect, useState } from 'react';

import { listTraps, positionGroups, setVideoPositions } from '../api';
import type { PositionGroup, Trap } from '../api';
import { MapPicker } from '../components/MapPicker';
import { formatDate } from '../format';

function num(raw: string): number | null {
  const t = raw.trim();
  if (t === '') return null;
  const n = Number(t.replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

/**
 * Ajuster la position des vidéos.
 *
 * Les vidéos sont regroupées par point : un lot de 400 captures partage presque
 * toujours la même position, et les corriger une par une n'aurait aucun sens.
 */
export function Positions({
  onError,
  since,
  onDone,
}: {
  onError: (e: string | null) => void;
  /** Ne montrer que les vidéos indexées depuis cette date, après une passe. */
  since?: string | null;
  onDone?: () => void;
}) {
  const [groups, setGroups] = useState<PositionGroup[] | null>(null);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [trapFilter, setTrapFilter] = useState('');
  const [editing, setEditing] = useState<PositionGroup | null>(null);
  const [lat, setLat] = useState<number | null>(null);
  const [lng, setLng] = useState<number | null>(null);
  const [alt, setAlt] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setGroups(await positionGroups(trapFilter || null, since ?? null));
      setTraps(await listTraps());
    } catch (e) {
      onError(String(e));
    }
  }, [onError, trapFilter, since]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const startEditing = (g: PositionGroup) => {
    setEditing(g);
    setLat(g.latitude);
    setLng(g.longitude);
    setAlt(null);
  };

  const save = async () => {
    if (!editing) return;
    onError(null);
    try {
      await setVideoPositions(editing.video_ids, lat, lng, alt);
      setEditing(null);
      await refresh();
      onDone?.();
    } catch (e) {
      onError(String(e));
    }
  };

  const trapOf = (id: string) => traps.find((t) => t.id === id);

  return (
    <div className="stack">
      <section className="panel stack">
        <div className="row row--flush">
          <h2>Position des vidéos</h2>
          <span className="app__spacer" />
          {!since && (
            <select value={trapFilter} onChange={(e) => setTrapFilter(e.target.value)}>
              <option value="">Tous les pièges</option>
              {traps.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          )}
        </div>

        <p className="muted small">
          Chaque vidéo garde <strong>sa propre</strong> position, copiée depuis son piège au
          moment de l’indexation. Déplacer un piège ne déplace donc jamais les captures déjà
          faites — seules les suivantes prendront la nouvelle position.
        </p>

        {groups === null ? (
          <p className="muted">Chargement…</p>
        ) : groups.length === 0 ? (
          <p className="muted">
            {since ? 'Aucune vidéo ajoutée lors de la dernière passe.' : 'Aucune vidéo indexée.'}
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Piège</th>
                <th>Position</th>
                <th className="num">Vidéos</th>
                <th>Période</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {groups.map((g, i) => (
                <tr key={`${g.trap_id}-${i}`}>
                  <td>{g.trap_name}</td>
                  <td className="mono muted">
                    {g.latitude != null && g.longitude != null ? (
                      <>
                        {g.latitude.toFixed(5)}, {g.longitude.toFixed(5)}
                        {g.any_manual && <span className="badge">ajustée</span>}
                      </>
                    ) : (
                      <span className="danger">sans position</span>
                    )}
                  </td>
                  <td className="num">{g.video_count}</td>
                  <td className="muted small">
                    {formatDate(g.first_at)} → {formatDate(g.last_at)}
                  </td>
                  <td className="row--actions">
                    <button onClick={() => startEditing(g)}>Ajuster…</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {editing && (
        <section className="panel stack">
          <h2>
            {editing.video_count} vidéo(s) — {editing.trap_name}
          </h2>

          <MapPicker
            latitude={lat}
            longitude={lng}
            onChange={(a, b) => {
              setLat(a);
              setLng(b);
            }}
          />

          <div className="fields">
            <label>
              Latitude
              <input value={lat ?? ''} onChange={(e) => setLat(num(e.target.value))} />
            </label>
            <label>
              Longitude
              <input value={lng ?? ''} onChange={(e) => setLng(num(e.target.value))} />
            </label>
            <label>
              Altitude (m)
              <input value={alt ?? ''} onChange={(e) => setAlt(num(e.target.value))} />
            </label>
          </div>

          <div className="row row--flush">
            <button className="primary" onClick={save}>
              Appliquer aux {editing.video_count} vidéo(s)
            </button>
            {(() => {
              const trap = trapOf(editing.trap_id);
              if (trap?.latitude == null || trap.longitude == null) return null;
              return (
                <button
                  onClick={() => {
                    setLat(trap.latitude);
                    setLng(trap.longitude);
                  }}
                >
                  Reprendre la position du piège
                </button>
              );
            })()}
            <button onClick={() => setEditing(null)}>Annuler</button>
          </div>

          <p className="muted small">
            Le lever et le coucher du soleil sont recalculés dans la foulée : le rythme
            d’activité et le filtre jour/nuit suivent la nouvelle position.
          </p>
        </section>
      )}
    </div>
  );
}
