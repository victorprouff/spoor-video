import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

import { forceOpen, moveData, restartApp, setDataDir } from '../api';
import { formatDateTime } from '../format';
import type { DataLocation } from '../api';
import { Confirm } from './Confirm';

type Pending = { kind: 'move' | 'open'; dir: string } | { kind: 'default' } | { kind: 'force' };

/**
 * Où vivent la base et les vignettes, et de quoi les déplacer. Sert dans l'onglet
 * Réglages, et seul à l'écran quand la base est introuvable au démarrage.
 */
export function DataLocationPanel({
  location,
  onError,
}: {
  location: DataLocation;
  onError: (e: string | null) => void;
}) {
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  const missing = location.error !== null;
  const locked = location.locked_by;

  const pick = async (kind: 'move' | 'open') => {
    const dir = await open({
      directory: true,
      title: kind === 'move' ? 'Dossier où déplacer la base' : 'Dossier contenant la base',
    });
    if (typeof dir === 'string') setPending({ kind, dir });
  };

  const apply = async () => {
    if (!pending) return;
    setBusy(true);
    onError(null);
    try {
      if (pending.kind === 'move') await moveData(pending.dir);
      else if (pending.kind === 'force') await forceOpen();
      else await setDataDir(pending.kind === 'open' ? pending.dir : null);
      await restartApp();
    } catch (e) {
      onError(String(e));
      setBusy(false);
      setPending(null);
    }
  };

  const confirm = pending && (
    <Confirm
      title={
        pending.kind === 'move'
          ? 'Déplacer la base ?'
          : pending.kind === 'open'
            ? 'Ouvrir cette base ?'
            : pending.kind === 'force'
              ? 'Ouvrir la base quand même ?'
              : 'Revenir à l’emplacement par défaut ?'
      }
      body={
        pending.kind === 'force'
          ? `À ne faire que si l’application est fermée sur « ${locked?.host} » : machine éteinte, ou application qui a planté. Si elle y est encore ouverte, kDrive gardera deux versions en conflit et le travail de l’une des deux machines sera perdu.`
          : pending.kind === 'move'
          ? `La base et les vignettes seront copiées dans ${pending.dir}, puis l’application redémarrera sur la copie. L’original reste en place dans ${location.dir} : tu pourras le supprimer une fois la copie vérifiée.`
          : pending.kind === 'open'
            ? `L’application redémarrera sur la base de ${pending.dir}. Rien n’est copié : la base actuelle reste où elle est, simplement plus utilisée.`
            : `L’application redémarrera sur la base de ${location.home}. S’il n’y en a pas, une base vide y sera créée.`
      }
      confirmLabel={
        busy
          ? 'Redémarrage…'
          : pending.kind === 'move'
            ? 'Copier et redémarrer'
            : pending.kind === 'force'
              ? 'Ouvrir quand même'
              : 'Redémarrer'
      }
      destructive={pending.kind === 'force'}
      onConfirm={apply}
      onCancel={() => !busy && setPending(null)}
    />
  );

  return (
    <section className="panel stack">
      <h2>Base de données</h2>

      {locked && (
        <div className="notice notice--danger">
          <p className="danger">
            <strong>La base est ouverte sur « {locked.host} »</strong>
            {locked.since && <> depuis le {formatDateTime(locked.since)}</>}.
          </p>
          <p className="muted small">
            L’ouvrir ici en même temps ferait perdre le travail de l’une des deux machines :
            kDrive garderait deux versions en conflit. Ferme l’application sur « {locked.host} »,
            laisse à kDrive le temps de synchroniser, puis réessaie.
          </p>
        </div>
      )}

      {missing && !locked && (
        <div className="notice notice--danger">
          <p className="danger">
            <strong>La base n’a pas pu être ouverte.</strong> {location.error}
          </p>
          <p className="muted small">
            Rien n’a été créé à la place, pour ne pas faire croire que le travail a disparu.
            Rebranche le disque puis relance, ou indique où se trouve la base.
          </p>
        </div>
      )}

      <dl className="facts">
        <dt>Dossier</dt>
        <dd className="mono small">
          {location.dir}
          {location.is_default && <span className="muted"> (par défaut)</span>}
        </dd>
        {location.dev && (
          <>
            <dt>Mode</dt>
            <dd>
              développement <span className="muted">— données séparées de l’application installée</span>
            </dd>
          </>
        )}
      </dl>

      <div className="row row--flush">
        {missing ? (
          <>
            <button className="primary" onClick={() => restartApp()}>
              Réessayer
            </button>
            {locked && (
              <button onClick={() => setPending({ kind: 'force' })}>Ouvrir quand même…</button>
            )}
          </>
        ) : (
          <button onClick={() => pick('move')}>Déplacer la base…</button>
        )}
        <button onClick={() => pick('open')}>Ouvrir une autre base…</button>
        {!location.is_default && (
          <button onClick={() => setPending({ kind: 'default' })}>
            Revenir à l’emplacement par défaut
          </button>
        )}
      </div>

      <p className="muted small">
        La base et les vignettes vivent ensemble dans ce dossier ; les vidéos, elles, restent
        sous la racine. Évite de placer la base <strong>dans</strong> la racine, où son dossier
        passerait pour un piège. Dans un dossier synchronisé (kDrive), la base peut servir à
        deux ordinateurs, mais pas en même temps : tant qu’elle est ouverte sur l’un, l’autre
        refuse de l’ouvrir. Ferme l’application avant de changer de machine.
      </p>

      {confirm}
    </section>
  );
}
