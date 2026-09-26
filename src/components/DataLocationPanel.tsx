import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

import { moveData, restartApp, setDataDir } from '../api';
import type { DataLocation } from '../api';
import { Confirm } from './Confirm';

type Pending = { kind: 'move' | 'open'; dir: string } | { kind: 'default' };

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
            : 'Revenir à l’emplacement par défaut ?'
      }
      body={
        pending.kind === 'move'
          ? `La base et les vignettes seront copiées dans ${pending.dir}, puis l’application redémarrera sur la copie. L’original reste en place dans ${location.dir} : tu pourras le supprimer une fois la copie vérifiée.`
          : pending.kind === 'open'
            ? `L’application redémarrera sur la base de ${pending.dir}. Rien n’est copié : la base actuelle reste où elle est, simplement plus utilisée.`
            : `L’application redémarrera sur la base de ${location.home}. S’il n’y en a pas, une base vide y sera créée.`
      }
      confirmLabel={busy ? 'Redémarrage…' : pending.kind === 'move' ? 'Copier et redémarrer' : 'Redémarrer'}
      destructive={false}
      onConfirm={apply}
      onCancel={() => !busy && setPending(null)}
    />
  );

  return (
    <section className="panel stack">
      <h2>Base de données</h2>

      {missing && (
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
          <button className="primary" onClick={() => restartApp()}>
            Réessayer
          </button>
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
        passerait pour un piège. Un dossier synchronisé (kDrive, iCloud) convient pour la
        sauvegarde, mais la base ne doit jamais être ouverte depuis deux Mac à la fois.
      </p>

      {confirm}
    </section>
  );
}
