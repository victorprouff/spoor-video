import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

import {
  CONFIDENCES,
  STATES,
  annotateSequences,
  listSequenceVideos,
  mergeSequences,
  splitSequence,
} from '../api';
import type { Confidence, GridTile, SequenceVideo, Species } from '../api';
import { formatDateTime } from '../format';

/**
 * Dépouillement plein écran, au clavier (§5).
 *
 * Une séquence à la fois, lecture en boucle, enchaînement automatique des vidéos du
 * groupe. Tout se fait sans quitter le clavier : c'est la vue du dépouillement fin,
 * là où la grille sert au tri grossier.
 */
export function Review({
  queue,
  startAt,
  species,
  onClose,
  onChanged,
  onError,
}: {
  queue: GridTile[];
  startAt: number;
  species: Species[];
  onClose: () => void;
  onChanged: () => void;
  onError: (e: string | null) => void;
}) {
  const [index, setIndex] = useState(startAt);
  const [videos, setVideos] = useState<SequenceVideo[] | null>(null);
  const [videoIndex, setVideoIndex] = useState(0);
  const [confidence, setConfidence] = useState<Confidence>('certain');
  const [picked, setPicked] = useState<Map<string, Confidence>>(new Map());
  const [flash, setFlash] = useState<string | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const videoRef = useRef<HTMLVideoElement>(null);

  const sequence = queue[index] as GridTile | undefined;

  // Les vidéos de la séquence courante, rechargées à chaque changement.
  useEffect(() => {
    if (!sequence) return;
    setVideos(null);
    setVideoIndex(0);
    setPicked(new Map());
    listSequenceVideos(sequence.id)
      .then(setVideos)
      .catch((e) => onError(String(e)));
  }, [sequence, onError]);

  const say = useCallback((message: string) => {
    setFlash(message);
    window.setTimeout(() => setFlash((f) => (f === message ? null : f)), 1400);
  }, []);

  const goTo = useCallback(
    (next: number) => {
      if (next < 0 || next >= queue.length) {
        say('Fin de la file');
        return;
      }
      setIndex(next);
    },
    [queue.length, say],
  );

  const playable = useMemo(
    () => (videos ?? []).filter((v) => v.file_state === 'present'),
    [videos],
  );
  const current = playable[videoIndex];

  /** Enchaîne les vidéos du groupe, puis reboucle sur la première. */
  const onEnded = () => {
    setVideoIndex((i) => (playable.length ? (i + 1) % playable.length : 0));
  };

  const validate = useCallback(
    async (annotationOverride?: Parameters<typeof annotateSequences>[1]) => {
      if (!sequence) return;
      const annotation =
        annotationOverride ??
        (picked.size > 0
          ? {
              add_species: [...picked].map(([species_id, conf]) => ({
                species_id,
                confidence: conf,
                count_min: null,
                count_max: null,
              })),
            }
          : null);

      if (!annotation) {
        say('Rien à valider — choisis une espèce, ou 0 pour « rien »');
        return;
      }
      try {
        await annotateSequences([sequence.id], annotation);
        onChanged();
        goTo(index + 1);
      } catch (e) {
        onError(String(e));
      }
    },
    [sequence, picked, index, goTo, onChanged, onError, say],
  );

  const toggleSpecies = useCallback(
    (id: string) => {
      const next = new Map(picked);
      if (next.has(id)) next.delete(id);
      else next.set(id, confidence);
      setPicked(next);
    },
    [picked, confidence],
  );

  const split = useCallback(async () => {
    if (!sequence || !current || videoIndex === 0) {
      say('Scinder demande d’être sur une autre vidéo que la première');
      return;
    }
    try {
      await splitSequence(sequence.id, current.id);
      say('Séquence scindée');
      onChanged();
      onClose();
    } catch (e) {
      onError(String(e));
    }
  }, [sequence, current, videoIndex, onChanged, onClose, onError, say]);

  const mergeWithPrevious = useCallback(async () => {
    const previous = queue[index + 1]; // la file est triée du plus récent au plus ancien
    if (!sequence || !previous) {
      say('Aucune séquence précédente');
      return;
    }
    if (previous.trap_id !== sequence.trap_id) {
      say('La précédente appartient à un autre piège');
      return;
    }
    try {
      await mergeSequences([sequence.id, previous.id]);
      say('Séquences fusionnées');
      onChanged();
      onClose();
    } catch (e) {
      onError(String(e));
    }
  }, [queue, index, sequence, onChanged, onClose, onError, say]);

  // Un seul gestionnaire pour tout le clavier : le mode n'a de sens que si rien
  // n'oblige à reprendre la souris.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const key = e.key;

      if (key === 'Escape') return onClose();
      if (key === '?') return setHelpOpen((h) => !h);

      if (key === ' ') {
        e.preventDefault();
        const v = videoRef.current;
        if (v) (v.paused ? v.play() : v.pause());
        return;
      }
      if (key === 'ArrowLeft') {
        e.preventDefault();
        return setVideoIndex((i) => Math.max(0, i - 1));
      }
      if (key === 'ArrowRight') {
        e.preventDefault();
        return setVideoIndex((i) => Math.min(playable.length - 1, i + 1));
      }
      if (key === 'ArrowUp') {
        e.preventDefault();
        return goTo(index - 1);
      }
      if (key === 'ArrowDown') {
        e.preventDefault();
        return goTo(index + 1);
      }
      if (key === 'Enter') {
        e.preventDefault();
        return void validate();
      }
      if (key === '0') {
        e.preventDefault();
        return void validate({ state: 'empty' });
      }

      const conf = CONFIDENCES.find((c) => c.key === key);
      if (conf) {
        e.preventDefault();
        setConfidence(conf.value);
        // La confiance choisie s'applique aussi à ce qui vient d'être coché :
        // on tape souvent l'espèce d'abord, la nuance ensuite.
        setPicked((p) => new Map([...p].map(([id]) => [id, conf.value])));
        return;
      }

      if (e.shiftKey && key.toLowerCase() === 's') {
        e.preventDefault();
        return void split();
      }
      if (e.shiftKey && key.toLowerCase() === 'f') {
        e.preventDefault();
        return void mergeWithPrevious();
      }
      if (e.shiftKey) return;

      const match = species.find((s) => s.shortcut_key === key.toLowerCase());
      if (match) {
        e.preventDefault();
        toggleSpecies(match.id);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    index,
    playable.length,
    species,
    goTo,
    validate,
    toggleSpecies,
    split,
    mergeWithPrevious,
    onClose,
  ]);

  if (!sequence) {
    return (
      <div className="review">
        <div className="review__done">
          <h2>File terminée</h2>
          <p className="muted">Plus rien à dépouiller avec ces filtres.</p>
          <button className="primary" onClick={onClose}>
            Revenir à la grille
          </button>
        </div>
      </div>
    );
  }

  const withShortcut = species.filter((s) => s.shortcut_key);

  return (
    <div className="review">
      <header className="review__bar">
        <strong>{sequence.trap_name}</strong>
        <span className="muted">{formatDateTime(sequence.started_at)}</span>
        <span className="muted">
          {sequence.video_count} vidéo(s)
          {current && playable.length > 1 && ` · ${videoIndex + 1}/${playable.length}`}
        </span>
        <span className="app__spacer" />
        <span className="muted">
          {index + 1} / {queue.length}
        </span>
        <button onClick={() => setHelpOpen(!helpOpen)}>Aide (?)</button>
        <button onClick={onClose}>Fermer (Échap)</button>
      </header>

      <div className="review__stage">
        {videos === null ? (
          <p className="muted">Chargement…</p>
        ) : current ? (
          <video
            ref={videoRef}
            key={current.id}
            src={convertFileSrc(current.file_path)}
            autoPlay
            loop={playable.length === 1}
            muted
            controls
            onEnded={onEnded}
            onError={() =>
              onError(
                `Lecture impossible : ${current.file_name}. ` +
                  `Le format n’est peut-être pas lisible par la fenêtre (AVI, codec exotique).`,
              )
            }
          />
        ) : (
          <div className="review__none">
            <p>Aucune vidéo lisible dans cette séquence.</p>
            <p className="muted small">
              Les fichiers ont été supprimés ou ont disparu — la donnée reste, pas l’image.
            </p>
          </div>
        )}
        {flash && <div className="review__flash">{flash}</div>}
      </div>

      <footer className="review__panel">
        <div className="review__row">
          <span className="muted small">Confiance</span>
          {CONFIDENCES.map((c) => (
            <button
              key={c.value}
              className={c.value === confidence ? 'tab--on' : undefined}
              onClick={() => setConfidence(c.value)}
            >
              <kbd>{c.key}</kbd> {c.label}
            </button>
          ))}
          <span className="actionbar__sep" />
          <button onClick={split} title="Maj+S">
            Scinder ici
          </button>
          <button onClick={mergeWithPrevious} title="Maj+F">
            Fusionner avec la précédente
          </button>
        </div>

        <div className="review__row review__species">
          {withShortcut.map((s) => (
            <button
              key={s.id}
              className={picked.has(s.id) ? 'tile--on' : undefined}
              onClick={() => toggleSpecies(s.id)}
            >
              <span className="swatch" style={{ background: s.color ?? 'transparent' }} />
              <kbd>{s.shortcut_key}</kbd> {s.common_name}
              {picked.has(s.id) && <em> · {picked.get(s.id)}</em>}
            </button>
          ))}
          {withShortcut.length === 0 && (
            <span className="muted small">
              Aucune espèce n’a de raccourci. Ajoute-les dans l’onglet Espèces.
            </span>
          )}
        </div>

        <div className="review__row">
          <button className="primary" onClick={() => void validate()} disabled={picked.size === 0}>
            <kbd>↵</kbd> Valider et suivante
          </button>
          <button onClick={() => void validate({ state: 'empty' })}>
            <kbd>0</kbd> Rien
          </button>
          {STATES.filter((s) => s.value !== 'empty').map((s) => (
            <button key={s.value} onClick={() => void validate({ state: s.value })}>
              {s.label}
            </button>
          ))}
          <span className="app__spacer" />
          <span className="muted small">↑ ↓ séquence · ← → vidéo · Espace lecture</span>
        </div>
      </footer>

      {helpOpen && <Help onClose={() => setHelpOpen(false)} species={withShortcut} />}
    </div>
  );
}

function Help({ onClose, species }: { onClose: () => void; species: Species[] }) {
  const rows: [string, string][] = [
    ['Espace', 'lecture / pause'],
    ['← →', 'vidéo précédente / suivante dans la séquence'],
    ['↑ ↓', 'séquence précédente / suivante'],
    ['Entrée', 'valider les espèces cochées et passer à la suivante'],
    ['0', 'rien à voir : valide et passe à la suivante'],
    ['1 2 3', 'certain / probable / possible'],
    ['Maj+S', 'scinder la séquence à la vidéo courante'],
    ['Maj+F', 'fusionner avec la séquence précédente'],
    ['Échap', 'fermer le mode plein écran'],
  ];
  return (
    <div className="review__help" onClick={onClose}>
      <div className="panel stack" onClick={(e) => e.stopPropagation()}>
        <h2>Raccourcis</h2>
        <dl className="facts">
          {rows.map(([k, v]) => (
            <div key={k} style={{ display: 'contents' }}>
              <dt>
                <kbd>{k}</kbd>
              </dt>
              <dd>{v}</dd>
            </div>
          ))}
        </dl>
        <h3>Espèces</h3>
        <div className="review__species">
          {species.map((s) => (
            <span key={s.id} className="chip">
              <kbd>{s.shortcut_key}</kbd> {s.common_name}
            </span>
          ))}
        </div>
        <button onClick={onClose}>Fermer</button>
      </div>
    </div>
  );
}
