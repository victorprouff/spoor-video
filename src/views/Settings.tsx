import { useCallback, useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';

import { appState, dataLocation, linkFolderToTrap, scanRoot, setRootPath } from '../api';
import type { AppState, DataLocation, ScanReport } from '../api';
import { DataLocationPanel } from '../components/DataLocationPanel';
import type { Theme } from '../theme';
import { formatDateTime } from '../format';
import { ScanReportPanel } from './Import';
import { SpeciesList } from './SpeciesList';
import { Traps } from './Traps';

const SECTIONS = [
  { id: 'folders', label: 'Dossiers et indexation' },
  { id: 'traps', label: 'Pièges' },
  { id: 'species', label: 'Espèces' },
  { id: 'appearance', label: 'Apparence' },
] as const;

type Section = (typeof SECTIONS)[number]['id'];

/** Ce qui ne sert que rarement : paramétrage et référentiels, hors de la barre d'onglets. */
export function Settings({
  theme,
  onTheme,
  onError,
  onChanged,
}: {
  theme: Theme;
  onTheme: () => void;
  onError: (e: string | null) => void;
  onChanged: () => void;
}) {
  const [section, setSection] = useState<Section>('folders');

  return (
    <div className="settings">
      <nav className="settings__nav">
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            className={s.id === section ? 'tab tab--on' : 'tab'}
            onClick={() => {
              onError(null);
              setSection(s.id);
            }}
          >
            {s.label}
          </button>
        ))}
      </nav>
      <div className="stack">
        {section === 'folders' && <Folders onError={onError} onChanged={onChanged} />}
        {section === 'traps' && <Traps onError={onError} />}
        {section === 'species' && <SpeciesList onError={onError} />}
        {section === 'appearance' && (
          <section className="panel stack">
            <h2>Apparence</h2>
            <div className="row row--flush">
              <span>Thème</span>
              <div className="chips">
                {(['light', 'dark'] as const).map((t) => (
                  <label key={t} className={theme === t ? 'chip chip--on' : 'chip'}>
                    <input
                      type="radio"
                      checked={theme === t}
                      onChange={() => theme !== t && onTheme()}
                    />
                    {t === 'light' ? 'Clair' : 'Sombre'}
                  </label>
                ))}
              </div>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

function Folders({
  onError,
  onChanged,
}: {
  onError: (e: string | null) => void;
  onChanged: () => void;
}) {
  const [state, setState] = useState<AppState | null>(null);
  const [location, setLocation] = useState<DataLocation | null>(null);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanning, setScanning] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setState(await appState());
      setLocation(await dataLocation());
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
      onChanged();
    } catch (e) {
      onError(String(e));
    }
  };

  const runScan = async () => {
    setScanning(true);
    onError(null);
    try {
      setReport(await scanRoot());
      await refresh();
      onChanged();
    } catch (e) {
      onError(String(e));
    } finally {
      setScanning(false);
    }
  };

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
    <>
      <section className="panel stack">
        <h2>Vidéos</h2>
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
              formatDateTime(state.last_scan)
            ) : (
              <span className="muted">jamais</span>
            )}{' '}
            <button onClick={runScan} disabled={!state?.root_path || scanning}>
              {scanning ? 'Indexation en cours…' : 'Relancer une passe'}
            </button>
          </dd>
        </dl>
        <p className="muted small">
          Une passe est rejouable sans dommage. Les nouveaux fichiers sont signalés d’eux-mêmes
          par l’onglet Dépouiller.
        </p>
      </section>

      {report && (
        <ScanReportPanel
          report={report}
          traps={state?.traps ?? []}
          onLink={linkFolder}
          onClose={() => setReport(null)}
        />
      )}

      {location && <DataLocationPanel location={location} onError={onError} />}
    </>
  );
}
