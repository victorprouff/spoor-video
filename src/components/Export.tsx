import { useState } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';

import { copyVideos, exportDetectionsCsv, exportSequencesCsv } from '../api';
import type { CopyReport, ExportReport, GridFilter } from '../api';

function humanBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} o`;
  const units = ['Ko', 'Mo', 'Go', 'To'];
  let value = bytes / 1024;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
}

/** Un nom de fichier daté : retrouver un export de la semaine dernière ne doit pas
    demander d'ouvrir chaque fichier. */
function defaultName(prefix: string): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return `spoor-${prefix}-${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}.csv`;
}

export function Export({
  filter,
  selection,
  total,
  onError,
  compact = false,
}: {
  filter: GridFilter;
  /** Séquences cochées dans la grille, pour la copie de fichiers. */
  selection: string[];
  total: number;
  onError: (e: string | null) => void;
  /** Sans la copie de fichiers, qui n'a de sens qu'avec une sélection de séquences. */
  compact?: boolean;
}) {
  const [report, setReport] = useState<ExportReport | null>(null);
  const [copy, setCopy] = useState<CopyReport | null>(null);
  const [busy, setBusy] = useState(false);

  const exportCsv = async (kind: 'sequences' | 'detections') => {
    const path = await save({
      title: kind === 'sequences' ? 'Exporter les séquences' : 'Exporter les détections',
      defaultPath: defaultName(kind === 'sequences' ? 'sequences' : 'detections'),
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    });
    if (!path) return;
    onError(null);
    setBusy(true);
    try {
      const fn = kind === 'sequences' ? exportSequencesCsv : exportDetectionsCsv;
      setCopy(null);
      setReport(await fn(filter, path));
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const copyFiles = async () => {
    const dir = await open({ directory: true, title: 'Copier les vidéos vers…' });
    if (typeof dir !== 'string') return;
    onError(null);
    setBusy(true);
    try {
      setReport(null);
      setCopy(await copyVideos(selection, dir));
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel stack">
      <h2>Exporter</h2>

      <div className="row row--flush">
        <button onClick={() => exportCsv('sequences')} disabled={busy || total === 0}>
          CSV des séquences
        </button>
        <button onClick={() => exportCsv('detections')} disabled={busy || total === 0}>
          CSV des détections
        </button>
        <span className="muted small">{total} séquence(s), filtres appliqués</span>
      </div>

      <details className="help">
        <summary>Quel fichier choisir ?</summary>
        <p className="help__body">
          Le CSV des <strong>séquences</strong> donne une ligne par passage, les espèces réunies
          dans une cellule : pratique à lire. Celui des <strong>détections</strong> donne une ligne
          par espèce, avec année, mois et heure dépliés : c’est la forme qu’attendent R et Python.
          Point-virgule et BOM, pour qu’Excel en français ouvre le fichier sans le massacrer.
        </p>
      </details>

      {!compact && (
        <div className="row row--flush">
          <button onClick={copyFiles} disabled={busy || selection.length === 0}>
            Copier les vidéos de {selection.length} séquence(s)
          </button>
          <span className="muted small">
            {selection.length === 0
              ? 'Coche des séquences dans la grille pour copier leurs fichiers'
              : 'Les originaux restent en place : l’application copie, elle ne déplace jamais'}
          </span>
        </div>
      )}

      {report && (
        <p>
          {report.rows} ligne(s) écrite(s) dans <span className="mono">{report.path}</span>
        </p>
      )}

      {copy && (
        <div className="stack">
          <p>
            {copy.copied} vidéo(s) copiée(s) ({humanBytes(copy.bytes)}) vers{' '}
            <span className="mono">{copy.dest}</span>
          </p>
          {copy.unavailable > 0 && (
            <p className="muted small">
              {copy.unavailable} vidéo(s) non copiée(s) : leur fichier a été supprimé ou a disparu.
              La donnée reste, pas l’image.
            </p>
          )}
          {copy.errors.length > 0 && (
            <ul className="danger small">
              {copy.errors.slice(0, 10).map((e, i) => (
                <li key={i} className="mono">
                  {e}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
