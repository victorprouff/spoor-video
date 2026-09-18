import { useCallback, useEffect, useState } from 'react';

import {
  createTrap,
  deleteTrap,
  linkFolderToTrap,
  listRootFolders,
  listTraps,
  updateTrap,
} from '../api';
import type { RootFolder, Trap, TrapInput } from '../api';
import { Confirm } from '../components/Confirm';
import { formatDate } from '../format';

const EMPTY: TrapInput = {
  name: '',
  folder_name: null,
  camera_name: null,
  latitude: null,
  longitude: null,
  altitude_m: null,
  clock_offset_minutes: 0,
  notes: null,
  active: true,
};

export function Traps({ onError }: { onError: (e: string | null) => void }) {
  const [traps, setTraps] = useState<Trap[]>([]);
  const [folders, setFolders] = useState<RootFolder[]>([]);
  const [editing, setEditing] = useState<{ id: string | null; input: TrapInput } | null>(null);
  const [confirming, setConfirming] = useState<Trap | null>(null);

  const refresh = useCallback(async () => {
    try {
      setTraps(await listTraps());
      setFolders(await listRootFolders());
    } catch (e) {
      onError(String(e));
    }
  }, [onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = async () => {
    if (!editing) return;
    onError(null);
    try {
      if (editing.id) await updateTrap(editing.id, editing.input);
      else await createTrap(editing.input);
      setEditing(null);
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const remove = async (trap: Trap) => {
    onError(null);
    try {
      await deleteTrap(trap.id);
      setConfirming(null);
      await refresh();
    } catch (e) {
      setConfirming(null);
      onError(String(e));
    }
  };

  const link = async (folder: string, trapId: string | null) => {
    onError(null);
    try {
      await linkFolderToTrap(folder, trapId);
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const unlinked = folders.filter((f) => !f.trap_name);

  return (
    <div className="stack">
      {unlinked.length > 0 && (
        <section className="panel stack">
          <h2>Dossiers non rattachés</h2>
          <p className="muted">
            Rien n’est importé de ces dossiers tant qu’ils ne désignent pas un piège. Aucun piège
            n’est créé automatiquement : une faute de frappe dans un nom de dossier en créerait un
            fantôme.
          </p>
          {unlinked.map((f) => (
            <div key={f.name} className="row">
              <span className="mono">{f.name}</span>
              <span className="app__spacer" />
              <button className="primary" onClick={() => link(f.name, null)}>
                Créer le piège « {f.name} »
              </button>
              {traps
                .filter((t) => !t.folder_name)
                .map((t) => (
                  <button key={t.id} onClick={() => link(f.name, t.id)}>
                    Rattacher à {t.name}
                  </button>
                ))}
            </div>
          ))}
        </section>
      )}

      <section className="panel stack">
        <div className="row row--flush">
          <h2>Pièges</h2>
          <span className="app__spacer" />
          <button className="primary" onClick={() => setEditing({ id: null, input: { ...EMPTY } })}>
            Nouveau piège
          </button>
        </div>

        {traps.length === 0 ? (
          <p className="muted">Aucun piège. Rattache un dossier, ou crée-en un à la main.</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Nom</th>
                <th>Dossier</th>
                <th>Position</th>
                <th>Horloge</th>
                <th className="num">Vidéos</th>
                <th>Période</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {traps.map((t) => (
                <tr key={t.id} className={t.active ? undefined : 'inactive'}>
                  <td>
                    {t.name}
                    {!t.active && <span className="muted"> (inactif)</span>}
                    {t.camera_name && <div className="muted small">caméra : {t.camera_name}</div>}
                  </td>
                  <td className="mono muted">{t.folder_name ?? '—'}</td>
                  <td className="mono muted">
                    {t.latitude != null && t.longitude != null
                      ? `${t.latitude.toFixed(5)}, ${t.longitude.toFixed(5)}`
                      : '—'}
                  </td>
                  <td className="num muted">
                    {t.clock_offset_minutes === 0 ? '—' : `${t.clock_offset_minutes > 0 ? '+' : ''}${t.clock_offset_minutes} min`}
                  </td>
                  <td className="num">{t.video_count}</td>
                  <td className="muted small">
                    {t.first_video_at ? (
                      <>
                        {formatDate(t.first_video_at)} → {formatDate(t.last_video_at)}
                      </>
                    ) : (
                      '—'
                    )}
                  </td>
                  <td className="row--actions">
                    <button onClick={() => setEditing({ id: t.id, input: toInput(t) })}>
                      Modifier
                    </button>
                    <button onClick={() => setConfirming(t)}>Supprimer</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {editing && (
        <TrapForm
          value={editing.input}
          isNew={!editing.id}
          onChange={(input) => setEditing({ ...editing, input })}
          onCancel={() => setEditing(null)}
          onSave={save}
        />
      )}

      {confirming && (
        <Confirm
          title={`Supprimer « ${confirming.name} » ?`}
          body={
            confirming.video_count > 0
              ? `${confirming.video_count} vidéo(s) y sont rattachées : la suppression sera refusée. Désactive-le plutôt.`
              : 'Ce piège ne porte aucune vidéo.'
          }
          confirmLabel="Supprimer"
          onConfirm={() => remove(confirming)}
          onCancel={() => setConfirming(null)}
        />
      )}
    </div>
  );
}

function toInput(t: Trap): TrapInput {
  return {
    name: t.name,
    folder_name: t.folder_name,
    camera_name: t.camera_name,
    latitude: t.latitude,
    longitude: t.longitude,
    altitude_m: t.altitude_m,
    clock_offset_minutes: t.clock_offset_minutes,
    notes: t.notes,
    active: t.active,
  };
}

/** Un champ vide vaut `null`, pas `0` : une altitude non saisie n'est pas le niveau de la mer. */
function num(raw: string): number | null {
  const trimmed = raw.trim();
  if (trimmed === '') return null;
  const n = Number(trimmed.replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

function TrapForm({
  value,
  isNew,
  onChange,
  onCancel,
  onSave,
}: {
  value: TrapInput;
  isNew: boolean;
  onChange: (v: TrapInput) => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  const set = <K extends keyof TrapInput>(key: K, v: TrapInput[K]) =>
    onChange({ ...value, [key]: v });

  return (
    <section className="panel stack">
      <h2>{isNew ? 'Nouveau piège' : `Modifier « ${value.name} »`}</h2>

      <div className="fields">
        <label>
          Nom
          <input value={value.name} onChange={(e) => set('name', e.target.value)} autoFocus />
        </label>
        <label>
          Dossier sous la racine
          <input
            value={value.folder_name ?? ''}
            onChange={(e) => set('folder_name', e.target.value || null)}
            placeholder="Mare basse"
          />
        </label>
        <label>
          Nom de la caméra
          <input
            value={value.camera_name ?? ''}
            onChange={(e) => set('camera_name', e.target.value || null)}
            placeholder="tel qu’écrit dans le bandeau"
          />
        </label>
        <label>
          Latitude
          <input
            value={value.latitude ?? ''}
            onChange={(e) => set('latitude', num(e.target.value))}
            placeholder="47.31245"
          />
        </label>
        <label>
          Longitude
          <input
            value={value.longitude ?? ''}
            onChange={(e) => set('longitude', num(e.target.value))}
            placeholder="4.08721"
          />
        </label>
        <label>
          Altitude (m)
          <input
            value={value.altitude_m ?? ''}
            onChange={(e) => set('altitude_m', num(e.target.value))}
          />
        </label>
        <label>
          Décalage d’horloge (min)
          <input
            value={value.clock_offset_minutes}
            onChange={(e) => set('clock_offset_minutes', num(e.target.value) ?? 0)}
          />
        </label>
        <label className="fields__wide">
          Notes
          <textarea
            rows={2}
            value={value.notes ?? ''}
            onChange={(e) => set('notes', e.target.value || null)}
          />
        </label>
        <label className="fields__check">
          <input
            type="checkbox"
            checked={value.active}
            onChange={(e) => set('active', e.target.checked)}
          />
          Piège actif
        </label>
      </div>

      <p className="muted small">
        Le décalage d’horloge corrige une caméra qui n’est pas à l’heure. Il s’applique à
        l’affichage : la date d’origine n’est jamais réécrite, donc une valeur fausse se corrige
        après coup sans réindexer.
      </p>
      <p className="muted small">
        Un piège déplacé de plusieurs centaines de mètres est un nouveau piège, pas le même avec de
        nouvelles coordonnées — sans quoi les statistiques par emplacement mentent.
      </p>

      <div className="row row--flush">
        <button className="primary" onClick={onSave}>
          Enregistrer
        </button>
        <button onClick={onCancel}>Annuler</button>
      </div>
    </section>
  );
}
