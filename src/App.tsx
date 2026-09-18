import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

import { useTheme } from './theme';

/** Miroir de `DbStatus` côté Rust. */
type DbStatus = {
  db_path: string;
  schema_version: number;
  species_count: number;
  traps_count: number;
  videos_count: number;
  sequences_count: number;
  root_path: string | null;
};

export default function App() {
  const { theme, toggle } = useTheme();
  const [status, setStatus] = useState<DbStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    setError(null);
    invoke<DbStatus>('db_status')
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(load, [load]);

  return (
    <div className="app">
      <header className="app__bar">
        <span className="app__title">Spoor Vidéo</span>
        <span className="app__spacer" />
        <button onClick={toggle} title="Changer de thème">
          {theme === 'dark' ? 'Thème clair' : 'Thème sombre'}
        </button>
      </header>

      <main className="app__main">
        <div className="panel stack">
          <div>
            <h1>Socle en place</h1>
            <p className="muted">
              Base SQLite ouverte et migrations appliquées au démarrage. L’étape suivante est
              l’indexation du dossier racine.
            </p>
          </div>

          {error && <p className="danger">Base indisponible : {error}</p>}

          {status && (
            <dl className="facts">
              <dt>Base</dt>
              <dd className="mono">{status.db_path}</dd>
              <dt>Version du schéma</dt>
              <dd>{status.schema_version}</dd>
              <dt>Racine des vidéos</dt>
              <dd>{status.root_path ?? <span className="muted">non configurée</span>}</dd>
              <dt>Pièges</dt>
              <dd>{status.traps_count}</dd>
              <dt>Séquences</dt>
              <dd>{status.sequences_count}</dd>
              <dt>Vidéos</dt>
              <dd>{status.videos_count}</dd>
              <dt>Espèces</dt>
              <dd>{status.species_count}</dd>
            </dl>
          )}

          <div>
            <button onClick={load}>Rafraîchir</button>
          </div>
        </div>
      </main>
    </div>
  );
}
