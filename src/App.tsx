import { useCallback, useEffect, useState } from 'react';

import { dataLocation, pendingVideos } from './api';
import type { DataLocation, PendingReport } from './api';
import { DataLocationPanel } from './components/DataLocationPanel';
import { useTheme } from './theme';
import { Grid } from './views/Grid';
import { Scan } from './views/Scan';
import { SpeciesList } from './views/SpeciesList';
import { Stats } from './views/Stats';
import { Traps } from './views/Traps';
import { Videos } from './views/Videos';

const TABS = [
  { id: 'stats', label: 'Statistiques' },
  { id: 'grid', label: 'Dépouillement' },
  { id: 'videos', label: 'Vidéos' },
  { id: 'scan', label: 'Indexation' },
  { id: 'traps', label: 'Pièges' },
  { id: 'species', label: 'Espèces' },
] as const;

type Tab = (typeof TABS)[number]['id'];

export default function App() {
  const { theme, toggle } = useTheme();
  // L'application s'ouvre sur ce qu'elle a à dire, pas sur ce qu'il reste à faire.
  const [tab, setTab] = useState<Tab>('stats');
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<PendingReport | null>(null);
  const [dismissed, setDismissed] = useState(false);
  // Base introuvable au démarrage : rien d'autre ne peut fonctionner, on n'affiche que
  // de quoi la retrouver.
  const [missingBase, setMissingBase] = useState<DataLocation | null>(null);

  useEffect(() => {
    dataLocation()
      .then((l) => setMissingBase(l.error ? l : null))
      .catch(() => undefined);
  }, []);

  const checkPending = useCallback(() => {
    pendingVideos()
      .then((r) => {
        setPending(r);
        // Un nouveau lot rouvre la notification, même si le précédent a été écarté.
        if (r.new_files === 0) setDismissed(false);
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    checkPending();
    // Une carte peut être copiée pendant que l'application est ouverte : on regarde
    // de temps en temps, et au retour au premier plan — jamais en bloquant l'interface.
    const timer = window.setInterval(checkPending, 120_000);
    const onFocus = () => checkPending();
    window.addEventListener('focus', onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('focus', onFocus);
    };
  }, [checkPending]);

  if (missingBase) {
    return (
      <div className="app">
        <header className="app__bar">
          <span className="app__title">Spoor Vidéo</span>
          <span className="app__spacer" />
          <button onClick={toggle}>{theme === 'dark' ? 'Thème clair' : 'Thème sombre'}</button>
        </header>
        <main className="app__main stack">
          {error && (
            <div className="panel notice notice--danger">
              <p className="danger">{error}</p>
            </div>
          )}
          <DataLocationPanel location={missingBase} onError={setError} />
        </main>
      </div>
    );
  }

  const go = (id: Tab) => {
    setError(null);
    setTab(id);
  };

  return (
    <div className="app">
      <header className="app__bar">
        <span className="app__title">Spoor Vidéo</span>
        <nav className="tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={t.id === tab ? 'tab tab--on' : 'tab'}
              onClick={() => go(t.id)}
            >
              {t.label}
              {t.id === 'scan' && !!pending?.new_files && (
                <span className="pip">{pending.new_files}</span>
              )}
            </button>
          ))}
        </nav>
        <span className="app__spacer" />
        <button onClick={toggle}>{theme === 'dark' ? 'Thème clair' : 'Thème sombre'}</button>
      </header>

      <main className="app__main stack">
        {!!pending?.new_files && !dismissed && tab !== 'scan' && (
          <div className="panel notice notice--new">
            <div className="row row--flush">
              <span>
                <strong>
                  {pending.new_files} fichier{pending.new_files > 1 ? 's' : ''} vidéo
                </strong>{' '}
                {pending.new_files > 1 ? 'attendent' : 'attend'} d’être indexé
                {pending.new_files > 1 ? 's' : ''}.
              </span>
              <span className="app__spacer" />
              <button className="primary" onClick={() => go('scan')}>
                Aller à l’indexation
              </button>
              <button onClick={() => setDismissed(true)}>Plus tard</button>
            </div>
            <p className="muted small">
              Compté d’après les chemins connus, sans lire les fichiers : une vidéo simplement
              renommée y apparaît comme nouvelle. La passe rétablira le compte exact.
            </p>
          </div>
        )}

        {error && (
          <div className="panel notice notice--danger">
            <p className="danger">{error}</p>
          </div>
        )}

        {tab === 'stats' && <Stats onError={setError} />}
        {tab === 'grid' && <Grid onError={setError} />}
        {tab === 'videos' && <Videos onError={setError} />}
        {tab === 'scan' && <Scan onError={setError} onScanned={checkPending} />}
        {tab === 'traps' && <Traps onError={setError} />}
        {tab === 'species' && <SpeciesList onError={setError} />}
      </main>
    </div>
  );
}
