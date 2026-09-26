import { useCallback, useEffect, useState } from 'react';

import {
  appState,
  discardedFiles,
  linkFolderToTrap,
  pendingVideos,
  restoreDiscarded,
  scanRoot,
} from '../api';
import type { AppState, DiscardedFile, PendingReport, ScanReport } from '../api';
import { formatDateTime } from '../format';

/**
 * Ce que l'import attend, en tête de l'onglet Dépouiller : fichiers à indexer, dossiers
 * non rattachés, vidéos écartées revenues, compte rendu de la dernière passe. **Ne montre
 * rien quand il n'y a rien à dire** : l'onglet n'est là que parce qu'il reste du travail.
 */
export function ImportPanel({
  onError,
  onChanged,
}: {
  onError: (e: string | null) => void;
  onChanged?: () => void;
}) {
  const [state, setState] = useState<AppState | null>(null);
  const [pending, setPending] = useState<PendingReport | null>(null);
  const [discarded, setDiscarded] = useState<DiscardedFile[]>([]);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanning, setScanning] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setState(await appState());
      setPending(await pendingVideos());
      setDiscarded(await discardedFiles());
    } catch (e) {
      onError(String(e));
    }
  }, [onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const runScan = useCallback(async () => {
    setScanning(true);
    onError(null);
    try {
      setReport(await scanRoot());
      await refresh();
      onChanged?.();
    } catch (e) {
      onError(String(e));
    } finally {
      setScanning(false);
    }
  }, [onError, refresh, onChanged]);

  const linkFolder = async (folder: string, trapId: string | null) => {
    onError(null);
    try {
      await linkFolderToTrap(folder, trapId);
      await runScan();
    } catch (e) {
      onError(String(e));
    }
  };

  const restore = async (hash: string) => {
    onError(null);
    try {
      await restoreDiscarded(hash);
      await refresh();
      onChanged?.();
    } catch (e) {
      onError(String(e));
    }
  };

  const newFiles = pending?.new_files ?? 0;
  const unlinked = pending?.unlinked ?? 0;

  return (
    <>
      {state && !state.ffmpeg_available && (
        <div className="panel notice notice--warn">
          <strong>ffmpeg est introuvable.</strong> Sans lui, ni date de capture ni vignette :
          l’indexation ne peut pas lire les vidéos.
        </div>
      )}

      {(newFiles > 0 || scanning) && (
        <div className="panel notice notice--new">
          <div className="row row--flush">
            <span>
              <strong>
                {newFiles} nouveau{newFiles > 1 ? 'x' : ''} fichier{newFiles > 1 ? 's' : ''}
              </strong>{' '}
              à indexer.
            </span>
            <span className="app__spacer" />
            <button className="primary" onClick={runScan} disabled={scanning}>
              {scanning ? 'Indexation en cours…' : 'Indexer'}
            </button>
          </div>
        </div>
      )}

      {unlinked > 0 && (
        <div className="panel notice notice--warn">
          <strong>
            {unlinked} vidéo{unlinked > 1 ? 's' : ''} dans un dossier non rattaché
          </strong>{' '}
          : {pending!.unlinked_folders.map((f) => `« ${f} »`).join(', ')}. Rien n’en sera importé
          tant qu’un piège ne réclame pas ces dossiers (Réglages → Pièges).
        </div>
      )}

      {report && (
        <ScanReportPanel
          report={report}
          traps={state?.traps ?? []}
          onLink={linkFolder}
          onClose={() => setReport(null)}
        />
      )}

      {discarded.length > 0 && <DiscardedPanel files={discarded} onRestore={restore} />}
    </>
  );
}

export function DiscardedPanel({
  files,
  onRestore,
}: {
  files: DiscardedFile[];
  onRestore: (hash: string) => void;
}) {
  return (
    <section className="panel stack">
      <h2>Vidéos écartées, de retour sur le disque</h2>
      <p className="muted small">
        Supprimées <strong>sans trace</strong>, leur fichier est de nouveau là (corbeille
        restaurée, ou dossier resynchronisé). L’indexation les ignore volontairement. Si l’une
        d’elles a été écartée par erreur, réintègre-la.
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
          {files.map((d) => (
            <tr key={d.content_hash}>
              <td className="mono small">
                {d.file_name}
                <div className="muted small">{d.file_path}</div>
              </td>
              <td className="muted small">{formatDateTime(d.purged_at)}</td>
              <td className="row--actions">
                <button onClick={() => onRestore(d.content_hash)}>Réintégrer</button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

/** Le compte rendu d'une passe, qu'on ferme une fois lu. */
export function ScanReportPanel({
  report,
  traps,
  onLink,
  onClose,
}: {
  report: ScanReport;
  traps: { id: string; name: string; folder_name: string | null }[];
  onLink: (folder: string, trapId: string | null) => void;
  onClose: () => void;
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
      <div className="row row--flush">
        <h2>Compte rendu de la passe</h2>
        <span className="app__spacer" />
        <button onClick={onClose}>Fermer</button>
      </div>
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
