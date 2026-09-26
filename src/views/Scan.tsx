import { useCallback, useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

import {
  appState,
  dataLocation,
  discardedFiles,
  linkFolderToTrap,
  pendingVideos,
  restoreDiscarded,
  scanRoot,
  setRootPath,
} from '../api';
import type { AppState, DataLocation, DiscardedFile, PendingReport, ScanReport } from '../api';
import { DataLocationPanel } from '../components/DataLocationPanel';
import { formatDateTime } from '../format';

export function Scan({
  onError,
  onScanned,
}: {
  onError: (e: string | null) => void;
  onScanned?: () => void;
}) {
  const [state, setState] = useState<AppState | null>(null);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [pending, setPending] = useState<PendingReport | null>(null);
  const [discarded, setDiscarded] = useState<DiscardedFile[]>([]);
  const [location, setLocation] = useState<DataLocation | null>(null);

  const refresh = useCallback(async () => {
    try {
      setState(await appState());
      setPending(await pendingVideos());
      setDiscarded(await discardedFiles());
      setLocation(await dataLocation());
    } catch (e) {
      onError(String(e));
    }
  }, [onError]);

  const restore = async (hash: string) => {
    onError(null);
    try {
      await restoreDiscarded(hash);
      await refresh();
      onScanned?.();
    } catch (e) {
      onError(String(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const chooseRoot = async () => {
    const chosen = await open({ directory: true, title: 'Dossier racine des vidéos' });
    if (typeof chosen !== 'string') return;
    onError(null);
    try {
      await setRootPath(chosen);
      setReport(null);
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const runScan = useCallback(async () => {
    setScanning(true);
    onError(null);
    try {
      const result = await scanRoot();
      setReport(result);
      await refresh();
      onScanned?.();
    } catch (e) {
      onError(String(e));
    } finally {
      setScanning(false);
    }
  }, [onError, refresh, onScanned]);

  const linkFolder = async (folder: string, trapId: string | null) => {
    onError(null);
    try {
      await linkFolderToTrap(folder, trapId);
      await runScan();
    } catch (e) {
      onError(String(e));
    }
  };

  return (
    <div className="stack">
      {state && !state.ffmpeg_available && (
        <div className="panel notice notice--warn">
          <strong>ffmpeg est introuvable.</strong> Sans lui, ni date de capture ni vignette :
          l’indexation ne peut pas lire les vidéos.
        </div>
      )}

      <section className="panel stack">
        <h2>Indexation</h2>
        <dl className="facts">
          <dt>Dossier racine</dt>
          <dd>
            {state?.root_path ?? <span className="muted">non configuré</span>}{' '}
            <button onClick={chooseRoot}>{state?.root_path ? 'Changer' : 'Choisir…'}</button>
          </dd>
          <dt>Vidéos indexées</dt>
          <dd>
            {state?.videos_count ?? 0}
            {!!state?.videos_missing && (
              <span className="muted"> · {state.videos_missing} disparues</span>
            )}
            {!!state?.videos_no_date && (
              <span className="muted"> · {state.videos_no_date} sans date</span>
            )}
          </dd>
          <dt>Dernière passe</dt>
          <dd>{state?.last_scan ? formatDateTime(state.last_scan) : <span className="muted">jamais</span>}</dd>
        </dl>
        <div>
          <button className="primary" onClick={runScan} disabled={!state?.root_path || scanning}>
            {scanning ? 'Indexation en cours…' : 'Lancer une passe'}
          </button>
        </div>
        <p className="muted small">
          Toute vidéo présente sous la racine et absente de la base est en attente de
          catégorisation. Une passe est rejouable sans dommage.
        </p>

        {pending && (pending.new_files > 0 || pending.unlinked > 0 || pending.discarded > 0) && (
          <ul className="tallies">
            <li className={pending.new_files ? undefined : 'muted'}>
              <span className="tallies__n">{pending.new_files}</span> à indexer
            </li>
            <li className={pending.unlinked ? undefined : 'muted'}>
              <span className="tallies__n">{pending.unlinked}</span> dans un dossier non rattaché
            </li>
            <li className={pending.discarded ? undefined : 'muted'}>
              <span className="tallies__n">{pending.discarded}</span> écartée(s) et revenue(s)
            </li>
          </ul>
        )}

        {pending && pending.unlinked > 0 && (
          <p className="muted small">
            Rien ne sera importé de {pending.unlinked_folders.map((f) => `« ${f} »`).join(', ')}{' '}
            tant qu’un piège ne réclame pas ces dossiers — voir l’onglet Pièges.
          </p>
        )}
      </section>

      {discarded.length > 0 && (
        <section className="panel stack">
          <h2>Vidéos écartées, de retour sur le disque</h2>
          <p className="muted small">
            Ces vidéos ont été supprimées <strong>sans trace</strong>, et leur fichier est de
            nouveau là — restauré depuis la corbeille, ou resynchronisé si la racine vit dans un
            dossier synchronisé. L’indexation les ignore volontairement : c’est ce qui évite que
            réimporter une carte SD fasse revenir les fausses déclenches. Si l’une d’elles a été
            écartée par erreur, réintègre-la.
          </p>
          <table className="table">
            <thead>
              <tr>
                <th>Fichier</th>
                <th>Écartée le</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {discarded.map((d) => (
                <tr key={d.content_hash}>
                  <td className="mono small">
                    {d.file_name}
                    <div className="muted small">{d.file_path}</div>
                  </td>
                  <td className="muted small">{formatDateTime(d.purged_at)}</td>
                  <td className="row--actions">
                    <button onClick={() => restore(d.content_hash)}>Réintégrer</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}

      {report && <Report report={report} traps={state?.traps ?? []} onLink={linkFolder} />}

      {location && <DataLocationPanel location={location} onError={onError} />}

    </div>
  );
}

function Report({
  report,
  traps,
  onLink,
}: {
  report: ScanReport;
  traps: { id: string; name: string; folder_name: string | null }[];
  onLink: (folder: string, trapId: string | null) => void;
}) {
  const lines: [string, number][] = [
    ['Fichiers vus', report.files_seen],
    ['Ajoutées', report.files_added],
    ['Déjà connues', report.files_known],
    ['Retrouvées ailleurs', report.files_moved],
    ['Revenues', report.files_recovered],
    ['Disparues', report.files_missing],
    ['Déjà écartées', report.files_repurged],
    ['Sans date', report.files_no_date],
    ['En erreur', report.files_error],
    ['Séquences reconstruites', report.sequences_built],
    ['Séquences intactes', report.sequences_frozen],
  ];

  return (
    <section className="panel stack">
      <h2>Compte rendu de la passe</h2>
      <ul className="tallies">
        {lines.map(([label, n]) => (
          <li key={label} className={n === 0 ? 'muted' : undefined}>
            <span className="tallies__n">{n}</span> {label}
          </li>
        ))}
      </ul>

      {report.files_no_date > 0 && (
        <p className="muted small">
          Une vidéo sans date exploitable n’est jamais datée d’office : elle attend une saisie
          manuelle.
        </p>
      )}

      {report.unknown_folders.length > 0 && (
        <div className="stack">
          <h3>Dossiers non rattachés</h3>
          <p className="muted small">Rien n’a été importé de ces dossiers.</p>
          {report.unknown_folders.map((folder) => (
            <div key={folder} className="row">
              <span className="mono">{folder}</span>
              <span className="app__spacer" />
              <button className="primary" onClick={() => onLink(folder, null)}>
                Créer le piège « {folder} »
              </button>
              {traps
                .filter((t) => !t.folder_name)
                .map((t) => (
                  <button key={t.id} onClick={() => onLink(folder, t.id)}>
                    Rattacher à {t.name}
                  </button>
                ))}
            </div>
          ))}
        </div>
      )}

      {report.errors.length > 0 && (
        <div className="stack">
          <h3>Erreurs</h3>
          <ul className="muted">
            {report.errors.slice(0, 20).map((e, i) => (
              <li key={i} className="mono small">
                {e}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
