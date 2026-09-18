import { useCallback, useEffect, useState } from 'react';

import { pendingVideos } from './api';
import { useTheme } from './theme';
import { Grid } from './views/Grid';
import { Positions } from './views/Positions';
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
  { id: 'positions', label: 'Positions' },
  { id: 'traps', label: 'Pièges' },
  { id: 'species', label: 'Espèces' },
] as const;

type Tab = (typeof TABS)[number]['id'];

export default function App() {
  const { theme, toggle } = useTheme();
  // L'application s'ouvre sur ce qu'elle a à dire, pas sur ce qu'il reste à faire.
  const [tab, setTab] = useState<Tab>('stats');
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(0);
  const [dismissed, setDismissed] = useState(false);

  const checkPending = useCallback(() => {
    pendingVideos()
      .then((n) => {
        setPending(n);
        // Un nouveau lot rouvre la notification, même si le précédent a été écarté.
        if (n === 0) setDismissed(false);
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
              {t.id === 'scan' && pending > 0 && <span className="pip">{pending}</span>}
            </button>
          ))}
        </nav>
        <span className="app__spacer" />
        <button onClick={toggle}>{theme === 'dark' ? 'Thème clair' : 'Thème sombre'}</button>
      </header>

      <main className="app__main stack">
        {pending > 0 && !dismissed && tab !== 'scan' && (
          <div className="panel notice notice--new">
            <div className="row row--flush">
              <span>
                <strong>
                  {pending} fichier{pending > 1 ? 's' : ''} vidéo
                </strong>{' '}
                {pending > 1 ? 'attendent' : 'attend'} d’être indexé{pending > 1 ? 's' : ''}.
              </span>
              <span className="app__spacer" />
              <button className="primary" onClick={() => go('scan')}>
                Aller à l’indexation
              </button>
              <button onClick={() => setDismissed(true)}>Plus tard</button>
            </div>
            <p className="muted small">
              Compté d’après les chemins connus, sans lire les fichiers : une vidéo simplement
              renommée y apparaît comme nouvelle. La passe d’indexation rétablira le compte exact.
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
        {tab === 'positions' && <Positions onError={setError} />}
        {tab === 'traps' && <Traps onError={setError} />}
        {tab === 'species' && <SpeciesList onError={setError} />}
      </main>
    </div>
  );
}
