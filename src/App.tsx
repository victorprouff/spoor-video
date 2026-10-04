import { useCallback, useEffect, useState } from 'react';

import { dataLocation, gridPage, pendingVideos } from './api';
import type { DataLocation, PendingReport, Rebase } from './api';
import { DataLocationPanel } from './components/DataLocationPanel';
import { EMPTY_FILTER } from './components/Filters';
import { useTheme } from './theme';
import { Grid } from './views/Grid';
import { ImportPanel } from './views/Import';
import { Settings } from './views/Settings';
import { Stats } from './views/Stats';
import { Videos } from './views/Videos';

type Tab = 'stats' | 'videos' | 'review' | 'settings';

/** Ce qui reste à faire après un import. L'onglet Dépouiller n'existe que si c'est non nul. */
type Work = { pending: PendingReport; unreviewed: number };

function workCount(w: Work | null): number {
  if (!w) return 0;
  return w.pending.new_files + w.pending.unlinked + w.pending.discarded + w.unreviewed;
}

function workTitle(w: Work): string {
  const parts: string[] = [];
  if (w.pending.new_files) parts.push(`${w.pending.new_files} fichier(s) à indexer`);
  if (w.unreviewed) parts.push(`${w.unreviewed} séquence(s) à dépouiller`);
  if (w.pending.unlinked) parts.push(`${w.pending.unlinked} vidéo(s) hors piège`);
  if (w.pending.discarded) parts.push(`${w.pending.discarded} vidéo(s) écartée(s) revenue(s)`);
  return parts.join(' · ');
}

function GearIcon() {
  return (
    <svg className="icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" aria-hidden="true">
      <path d="M13.22 7.07 L14.95 7.18 L14.95 8.82 L13.22 8.93 L12.35 11.03 L13.50 12.33 L12.33 13.50 L11.03 12.35 L8.93 13.22 L8.82 14.95 L7.18 14.95 L7.07 13.22 L4.97 12.35 L3.67 13.50 L2.50 12.33 L3.65 11.03 L2.78 8.93 L1.05 8.82 L1.05 7.18 L2.78 7.07 L3.65 4.97 L2.50 3.67 L3.67 2.50 L4.97 3.65 L7.07 2.78 L7.18 1.05 L8.82 1.05 L8.93 2.78 L11.03 3.65 L12.33 2.50 L13.50 3.67 L12.35 4.97 Z" strokeLinejoin="round" />
      <circle cx="8" cy="8" r="2.2" />
    </svg>
  );
}

export default function App() {
  const { theme, toggle } = useTheme();
  // L'application s'ouvre sur ce qu'elle a à dire, pas sur ce qu'il reste à faire.
  const [tab, setTab] = useState<Tab>('stats');
  const [error, setError] = useState<string | null>(null);
  const [work, setWork] = useState<Work | null>(null);
  const [focusSequence, setFocusSequence] = useState<string | null>(null);
  // Base introuvable au démarrage : rien d'autre ne peut fonctionner, on n'affiche que
  // de quoi la retrouver.
  const [missingBase, setMissingBase] = useState<DataLocation | null>(null);
  // Base venue de l'autre ordinateur : ses chemins ont été réécrits à l'ouverture. On le
  // dit, plutôt que de le faire en silence.
  const [rebased, setRebased] = useState<Rebase | null>(null);

  useEffect(() => {
    dataLocation()
      .then((l) => {
        setMissingBase(l.error ? l : null);
        setRebased(l.root_rebased);
      })
      .catch(() => undefined);
  }, []);

  const checkWork = useCallback(() => {
    Promise.all([pendingVideos(), gridPage({ ...EMPTY_FILTER, limit: 1 })])
      .then(([pending, page]) => setWork({ pending, unreviewed: page.unreviewed_total }))
      .catch(() => undefined);
  }, []);

  // Après une annotation, seul le nombre de séquences à dépouiller bouge : inutile de
  // reparcourir la racine, qui peut vivre sur un disque lent ou synchronisé.
  const checkUnreviewed = useCallback(() => {
    gridPage({ ...EMPTY_FILTER, limit: 1 })
      .then((page) => setWork((w) => (w ? { ...w, unreviewed: page.unreviewed_total } : w)))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    checkWork();
    // Une carte peut être copiée pendant que l'application est ouverte : on regarde
    // de temps en temps, et au retour au premier plan — jamais en bloquant l'interface.
    const timer = window.setInterval(checkWork, 120_000);
    const onFocus = () => checkWork();
    window.addEventListener('focus', onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('focus', onFocus);
    };
  }, [checkWork]);

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
    if (id !== 'review') setFocusSequence(null);
  };

  const openSequence = (sequenceId: string) => {
    setError(null);
    setFocusSequence(sequenceId);
    setTab('review');
  };

  // L'onglet ne disparaît pas sous les yeux : on le garde tant qu'on y est.
  const remaining = workCount(work);
  const showReview = remaining > 0 || tab === 'review';
  const tabs: { id: Tab; label: string }[] = [
    { id: 'stats', label: 'Statistiques' },
    { id: 'videos', label: 'Vidéos' },
    ...(showReview ? [{ id: 'review' as Tab, label: 'Dépouiller' }] : []),
  ];

  // La pastille dit d'abord ce qu'une passe ferait (fichiers à indexer), sinon ce qu'il
  // reste à dépouiller. Le détail est dans l'infobulle et en tête de l'onglet.
  const newFiles = work?.pending.new_files ?? 0;
  const pip = newFiles || work?.unreviewed || (remaining > 0 ? '!' : null);

  return (
    <div className="app">
      <header className="app__bar">
        <span className="app__title">Spoor Vidéo</span>
        <nav className="tabs">
          {tabs.map((t) => (
            <button
              key={t.id}
              className={t.id === tab ? 'tab tab--on' : 'tab'}
              onClick={() => go(t.id)}
              title={t.id === 'review' && work ? workTitle(work) : undefined}
            >
              {t.label}
              {t.id === 'review' && pip && (
                <span className={newFiles ? 'pip' : 'pip pip--quiet'}>{pip}</span>
              )}
            </button>
          ))}
        </nav>
        <span className="app__spacer" />
        <button
          className={tab === 'settings' ? 'tab tab--on tab--icon' : 'tab tab--icon'}
          onClick={() => go('settings')}
          title="Réglages"
          aria-label="Réglages"
        >
          <GearIcon />
        </button>
      </header>

      <main className="app__main stack">
        {error && (
          <div className="panel notice notice--danger">
            <p className="danger">{error}</p>
          </div>
        )}
        {rebased && (
          <div className="panel notice stack">
            <p>
              Base venue de l’autre ordinateur : les chemins des vidéos ont été adaptés à
              celui-ci ({rebased.from} → {rebased.to}). {rebased.found} vidéo
              {rebased.found > 1 ? 's' : ''} retrouvée{rebased.found > 1 ? 's' : ''} sur{' '}
              {rebased.total}.
            </p>
            <div className="row row--flush">
              <button onClick={() => setRebased(null)}>Compris</button>
            </div>
          </div>
        )}

        {tab === 'stats' && <Stats onError={setError} />}
        {tab === 'videos' && <Videos onError={setError} onOpenSequence={openSequence} />}
        {tab === 'review' && (
          <>
            {!focusSequence && <ImportPanel onError={setError} onChanged={checkWork} />}
            <Grid
              onError={setError}
              onChanged={checkUnreviewed}
              focusSequenceId={focusSequence}
              onClearFocus={() => setFocusSequence(null)}
            />
          </>
        )}
        {tab === 'settings' && (
          <Settings theme={theme} onTheme={toggle} onError={setError} onChanged={checkWork} />
        )}
      </main>
    </div>
  );
}
