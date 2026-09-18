import { useCallback, useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

import { appState, linkFolderToTrap, scanRoot, setRootPath } from '../api';
import type { AppState, ScanReport } from '../api';
import { formatDateTime } from '../format';
import { Positions } from './Positions';

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
  const [scannedAt, setScannedAt] = useState<string | null>(null);
  const [checkPositions, setCheckPositions] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setState(await appState());
    } catch (e) {
      onError(String(e));
    }
  }, [onError]);

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
      setScannedAt(new Date().toISOString());
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
      </section>

      {report && <Report report={report} traps={state?.traps ?? []} onLink={linkFolder} />}

      {report && report.files_added > 0 && !checkPositions && (
        <section className="panel">
          <div className="row row--flush">
            <span>
              {report.files_added} vidéo(s) ajoutée(s) ont pris la position de leur piège.
            </span>
            <span className="app__spacer" />
            <button className="primary" onClick={() => setCheckPositions(true)}>
              Vérifier leur position
            </button>
          </div>
        </section>
      )}

      {checkPositions && scannedAt && (
        <Positions onError={onError} since={scannedAt} onDone={() => undefined} />
      )}
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
