import { useEffect, useRef, useState } from 'react';

import {
  deleteVideosKeepingTrace,
  deleteVideosWithoutTrace,
  previewDeletion,
  videosOfSequences,
} from '../api';
import type { DeletePreview, DeleteReport } from '../api';

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

/**
 * La seule opération irréversible de l'application. La boîte annonce donc des chiffres
 * mesurés, pas estimés, et le focus va sur « Annuler » — jamais sur l'action destructive.
 */
export function DeleteDialog({
  sequenceIds,
  onDone,
  onCancel,
  onError,
}: {
  sequenceIds: string[];
  onDone: (report: DeleteReport, mode: 'trace' | 'purge') => void;
  onCancel: () => void;
  onError: (e: string | null) => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [videoIds, setVideoIds] = useState<string[] | null>(null);
  const [preview, setPreview] = useState<DeletePreview | null>(null);
  const [reason, setReason] = useState('');
  const [mode, setMode] = useState<'trace' | 'purge'>('trace');
  const [confirmText, setConfirmText] = useState('');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    ref.current?.showModal();
    cancelRef.current?.focus();
    videosOfSequences(sequenceIds)
      .then(async (ids) => {
        setVideoIds(ids);
        setPreview(await previewDeletion(ids));
      })
      .catch((e) => onError(String(e)));
  }, [sequenceIds, onError]);

  const run = async () => {
    if (!videoIds) return;
    setBusy(true);
    onError(null);
    try {
      const report =
        mode === 'trace'
          ? await deleteVideosKeepingTrace(videoIds, reason || null)
          : await deleteVideosWithoutTrace(videoIds);
      onDone(report, mode);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // Effacer une identification est un geste qu'on ne doit pas pouvoir faire distraitement.
  const needsTyping = mode === 'purge' && (preview?.species_annotations ?? 0) > 0;
  const ready = !!preview && preview.videos > 0 && (!needsTyping || confirmText === 'supprimer');

  return (
    <dialog ref={ref} className="dialog dialog--wide" onCancel={onCancel} onClose={onCancel}>
      <h2>Supprimer des vidéos</h2>

      {!preview ? (
        <p className="muted">Calcul…</p>
      ) : (
        <>
          <dl className="facts">
            <dt>Vidéos</dt>
            <dd>
              {preview.videos} dans {preview.sequences} séquence(s)
            </dd>
            <dt>Fichiers à envoyer à la corbeille</dt>
            <dd>
              {preview.present_files} ({humanBytes(preview.total_bytes)})
              {preview.already_gone > 0 && (
                <span className="muted"> · {preview.already_gone} déjà absent(s)</span>
              )}
            </dd>
            {preview.species_annotations > 0 && (
              <>
                <dt>Identifications</dt>
                <dd>{preview.species_annotations} espèce(s) identifiée(s)</dd>
              </>
            )}
          </dl>

          {preview.outside_root > 0 && (
            <p className="notice notice--warn panel small">
              {preview.outside_root} fichier(s) sont hors du dossier racine et ne seront{' '}
              <strong>pas</strong> touchés. L’application ne supprime rien en dehors de la racine
              configurée.
            </p>
          )}

          <fieldset className="stack">
            <legend>Que garder ?</legend>

            <label className="choice">
              <input
                type="radio"
                checked={mode === 'trace'}
                onChange={() => setMode('trace')}
              />
              <span>
                <strong>Garder la trace</strong> — le fichier part à la corbeille, la ligne reste :
                date, piège, espèce, tags. La séquence compte encore dans les statistiques, seule la
                lecture devient impossible. <em>C’est le cas courant : un passage humain, un chien.</em>
              </span>
            </label>

            <label className="choice">
              <input
                type="radio"
                checked={mode === 'purge'}
                onChange={() => setMode('purge')}
              />
              <span>
                <strong>Ne rien garder</strong> — le fichier <em>et</em> la ligne partent.
                L’empreinte est mémorisée pour que réimporter la même carte ne fasse pas revenir ces
                vidéos. <em>À réserver à ce qui n’a rien à dire : herbe, pluie, image vide.</em>
              </span>
            </label>
          </fieldset>

          {mode === 'trace' && (
            <label className="stack">
              Raison (facultative)
              <input
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                placeholder="passage humain, promeneur, tracteur…"
              />
            </label>
          )}

          {mode === 'purge' && preview.reviewed_sequences > 0 && (
            <p className="notice notice--warn panel small">
              {preview.reviewed_sequences} séquence(s) déjà dépouillée(s) dans cette sélection.
              Sans trace, leur dépouillement est perdu — pas seulement les vidéos.
            </p>
          )}

          {needsTyping && (
            <label className="stack">
              <span className="danger">
                {preview.species_annotations} identification(s) vont disparaître. Écris{' '}
                <code>supprimer</code> pour confirmer.
              </span>
              <input value={confirmText} onChange={(e) => setConfirmText(e.target.value)} />
            </label>
          )}

          <p className="muted small">
            Les fichiers vont à la <strong>corbeille du système</strong>, pas au néant : ils restent
            récupérables tant que tu ne l’as pas vidée. L’application ne vide jamais la corbeille.
          </p>
        </>
      )}

      <div className="row row--flush">
        <button ref={cancelRef} onClick={onCancel} disabled={busy}>
          Annuler
        </button>
        <span className="app__spacer" />
        <button className="destructive" onClick={run} disabled={!ready || busy}>
          {busy
            ? 'Suppression…'
            : mode === 'trace'
              ? 'Supprimer en gardant la trace'
              : 'Supprimer sans rien garder'}
        </button>
      </div>
    </dialog>
  );
}
