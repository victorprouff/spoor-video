import { useState } from 'react';

import { useTheme } from './theme';
import { Grid } from './views/Grid';
import { Scan } from './views/Scan';
import { Sequences } from './views/Sequences';
import { SpeciesList } from './views/SpeciesList';
import { Traps } from './views/Traps';

const TABS = [
  { id: 'grid', label: 'Dépouillement' },
  { id: 'scan', label: 'Indexation' },
  { id: 'sequences', label: 'Séquences' },
  { id: 'traps', label: 'Pièges' },
  { id: 'species', label: 'Espèces' },
] as const;

type Tab = (typeof TABS)[number]['id'];

export default function App() {
  const { theme, toggle } = useTheme();
  const [tab, setTab] = useState<Tab>('grid');
  const [error, setError] = useState<string | null>(null);

  return (
    <div className="app">
      <header className="app__bar">
        <span className="app__title">Spoor Vidéo</span>
        <nav className="tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={t.id === tab ? 'tab tab--on' : 'tab'}
              onClick={() => {
                setError(null);
                setTab(t.id);
              }}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <span className="app__spacer" />
        <button onClick={toggle}>{theme === 'dark' ? 'Thème clair' : 'Thème sombre'}</button>
      </header>

      <main className="app__main stack">
        {error && (
          <div className="panel notice notice--danger">
            <p className="danger">{error}</p>
          </div>
        )}

        {tab === 'grid' && <Grid onError={setError} />}
        {tab === 'scan' && <Scan onError={setError} />}
        {tab === 'sequences' && <Sequences onError={setError} />}
        {tab === 'traps' && <Traps onError={setError} />}
        {tab === 'species' && <SpeciesList onError={setError} />}
      </main>
    </div>
  );
}
