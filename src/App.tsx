import { useCallback, useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

import { appState, linkFolderToTrap, scanRoot, setRootPath } from './api';
import type { AppState, ScanReport } from './api';
import { useTheme } from './theme';

export default function App() {
  const { theme, toggle } = useTheme();
  const [state, setState] = useState<AppState | null>(null);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setState(await appState());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const chooseRoot = async () => {
    const chosen = await open({ directory: true, title: 'Dossier racine des vidéos' });
    if (typeof chosen !== 'string') return;
    setError(null);
    try {
      await setRootPath(chosen);
      setReport(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const runScan = async () => {
    setScanning(true);
    setError(null);
    try {
      setReport(await scanRoot());
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const linkFolder = async (folder: string, trapId: string | null) => {
    try {
      await linkFolderToTrap(folder, trapId);
      await refresh();
      await runScan();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="app">
      <header className="app__bar">
        <span className="app__title">Spoor Vidéo</span>
        <span className="app__spacer" />
        <button onClick={toggle}>{theme === 'dark' ? 'Thème clair' : 'Thème sombre'}</button>
      </header>

      <main className="app__main stack">
        {error && (
          <div className="panel">
            <p className="danger">{error}</p>
          </div>
        )}

        {state && !state.ffmpeg_available && (
          <div className="panel notice notice--warn">
            <strong>ffmpeg est introuvable.</strong> Sans lui, ni date de capture ni vignette :
            l’indexation ne peut pas lire les vidéos.
          </div>
        )}

        <section className="panel stack">
          <h1>Indexation</h1>
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
            <dd>
              {state?.last_scan ? (
                new Date(state.last_scan).toLocaleString('fr-FR')
              ) : (
                <span className="muted">jamais</span>
              )}
            </dd>
          </dl>
          <div>
            <button className="primary" onClick={runScan} disabled={!state?.root_path || scanning}>
              {scanning ? 'Indexation en cours…' : 'Lancer une passe'}
            </button>
          </div>
          <p className="muted">
            Toute vidéo présente sous la racine et absente de la base est en attente de
            catégorisation. Une passe est rejouable sans dommage.
          </p>
        </section>

        {report && <Report report={report} traps={state?.traps ?? []} onLink={linkFolder} />}

        {!!state?.traps.length && (
          <section className="panel stack">
            <h2>Pièges</h2>
            <table className="table">
              <thead>
                <tr>
                  <th>Nom</th>
                  <th>Dossier</th>
                  <th className="num">Vidéos</th>
                </tr>
              </thead>
              <tbody>
                {state.traps.map((t) => (
                  <tr key={t.id}>
                    <td>{t.name}</td>
                    <td className="mono muted">{t.folder_name ?? '—'}</td>
                    <td className="num">{t.video_count}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}
      </main>
    </div>
  );
}

function Report({
  report,
  traps,
  onLink,
}: {
  report: ScanReport;
  traps: { id: string; name: string }[];
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
        <p className="muted">
          Une vidéo sans date exploitable n’est jamais datée d’office : elle attend une saisie
          manuelle.
        </p>
      )}

      {report.unknown_folders.length > 0 && (
        <div className="stack">
          <h3>Dossiers non rattachés</h3>
          <p className="muted">
            Rien n’a été importé de ces dossiers. Aucun piège n’est créé automatiquement — une
            faute de frappe dans un nom de dossier en créerait un fantôme.
          </p>
          {report.unknown_folders.map((folder) => (
            <div key={folder} className="row">
              <span className="mono">{folder}</span>
              <span className="app__spacer" />
              <button className="primary" onClick={() => onLink(folder, null)}>
                Créer le piège « {folder} »
              </button>
              {traps.map((t) => (
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
              <li key={i} className="mono">
                {e}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
