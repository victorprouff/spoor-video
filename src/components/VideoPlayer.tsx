import { useCallback, useEffect, useRef, useState } from 'react';
import { videoSrc } from '../videoSrc';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  CloseIcon,
  FullscreenIcon,
  NextIcon,
  PrevIcon,
  SoundIcon,
  StarIcon,
} from './icons';

/** Le même réglage de son que le mode plein écran : un seul choix, partout. */
function storedMuted(): boolean {
  try {
    return localStorage.getItem('video-muted') === '1';
  } catch {
    return false;
  }
}

/**
 * Le plein écran passe par la fenêtre du système, pas par l'API du navigateur :
 * WKWebView, qui affiche l'application, n'expose pas `requestFullscreen` — le clic ne
 * faisait rien du tout. La fenêtre s'agrandit donc côté système et le lecteur occupe
 * toute sa surface en CSS ; les commandes et les flèches restent les nôtres.
 */
async function setWindowFullscreen(on: boolean): Promise<void> {
  try {
    await getCurrentWindow().setFullscreen(on);
  } catch {
    /* hors Tauri (page d'aperçu) : seul le dépliage CSS a lieu */
  }
}

export function VideoPlayer({
  filePath,
  title,
  subtitle,
  onClose,
  onPrev,
  onNext,
  position,
  onReview,
  favorite,
  onToggleFavorite,
}: {
  filePath: string;
  title: string;
  subtitle?: string;
  onClose: () => void;
  /** Absents quand il n'y a rien avant ou après : les flèches ne font alors rien. */
  onPrev?: () => void;
  onNext?: () => void;
  /** « 3 / 48 » : savoir où l'on en est quand on défile. */
  position?: string;
  /** Rouvre le passage de la vidéo au dépouillement, pour le corriger. */
  onReview?: () => void;
  /** État favori de la vidéo affichée ; le bouton n'apparaît que si on peut le changer. */
  favorite?: boolean;
  onToggleFavorite?: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [muted, setMuted] = useState(storedMuted);
  const [failed, setFailed] = useState(false);
  const [full, setFull] = useState(false);

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  // Le fichier change quand on défile : la vidéo précédente pouvait être illisible,
  // on redonne sa chance à la suivante.
  useEffect(() => setFailed(false), [filePath]);

  useEffect(() => {
    try {
      localStorage.setItem('video-muted', muted ? '1' : '0');
    } catch {
      /* sans conséquence : le choix tient pour la session */
    }
  }, [muted]);

  const toggleFull = useCallback(() => {
    setFull((f) => {
      void setWindowFullscreen(!f);
      return !f;
    });
  }, []);

  // Fermer le lecteur rend toujours la fenêtre : sans cela l'application resterait
  // plein écran sur la liste.
  const close = useCallback(() => {
    if (full) void setWindowFullscreen(false);
    onClose();
  }, [full, onClose]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      if (e.key === 'ArrowRight') {
        e.preventDefault();
        return onNext?.();
      }
      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        return onPrev?.();
      }
      if (e.key === ' ') {
        e.preventDefault();
        const v = ref.current?.querySelector('video');
        if (v) (v.paused ? void v.play() : v.pause());
        return;
      }
      // Échap rend d'abord la fenêtre, et ne ferme qu'ensuite : sortir du plein écran
      // et perdre la vidéo d'un seul coup serait brutal.
      if (e.key === 'Escape') {
        e.preventDefault();
        return full ? toggleFull() : close();
      }
      // Avant « F » seul : le test de la touche ignore la casse, Maj+F ouvrirait sinon
      // le plein écran au lieu de marquer la vidéo.
      if (e.shiftKey && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        return onToggleFavorite?.();
      }
      if (e.key.toLowerCase() === 'f') {
        e.preventDefault();
        return toggleFull();
      }
      if (e.shiftKey && e.key.toLowerCase() === 'm') {
        e.preventDefault();
        setMuted((m) => !m);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [close, full, onNext, onPrev, onToggleFavorite, toggleFull]);

  return (
    <dialog
      ref={ref}
      className="dialog dialog--player"
      onCancel={(e) => {
        // Le clavier est déjà géré plus haut : ici on empêche seulement la boîte de
        // se fermer d'elle-même quand Échap ne doit que quitter le plein écran.
        if (full) {
          e.preventDefault();
          return;
        }
        close();
      }}
      onClose={onClose}
    >
      <div className={full ? 'player player--full' : 'player'}>
        <div className="row row--flush">
          <strong>{title}</strong>
          {position && <span className="muted small">{position}</span>}
          <span className="app__spacer" />
          <button
            className="icon-button"
            onClick={() => onPrev?.()}
            disabled={!onPrev}
            aria-label="Vidéo précédente"
            title="Vidéo précédente (←)"
          >
            <PrevIcon />
          </button>
          <button
            className="icon-button"
            onClick={() => onNext?.()}
            disabled={!onNext}
            aria-label="Vidéo suivante"
            title="Vidéo suivante (→)"
          >
            <NextIcon />
          </button>
          <button
            className="icon-button"
            onClick={() => setMuted((m) => !m)}
            aria-label={muted ? 'Rétablir le son' : 'Couper le son'}
            title={muted ? 'Son coupé — rétablir (Maj+M)' : 'Son actif — couper (Maj+M)'}
          >
            <SoundIcon muted={muted} />
          </button>
          {onToggleFavorite && (
            <button
              className={favorite ? 'icon-button star-button star-button--on' : 'icon-button star-button'}
              onClick={onToggleFavorite}
              aria-pressed={!!favorite}
              aria-label="Favorite"
              title={favorite ? 'Favorite — retirer (Maj+F)' : 'Marquer comme favorite (Maj+F)'}
            >
              <StarIcon filled={!!favorite} />
            </button>
          )}
          <button
            className="icon-button"
            onClick={toggleFull}
            aria-label={full ? 'Quitter le plein écran' : 'Plein écran'}
            title={full ? 'Quitter le plein écran (F ou Échap)' : 'Plein écran (F)'}
          >
            <FullscreenIcon exit={full} />
          </button>
          {onReview && (
            <button
              onClick={() => {
                close();
                onReview();
              }}
              title="Rouvrir son passage au dépouillement pour corriger espèce, état ou découpage"
            >
              Dépouiller
            </button>
          )}
          <button className="icon-button" onClick={close} aria-label="Fermer" title="Fermer (Échap)">
            <CloseIcon />
          </button>
        </div>

        {failed ? (
          <p className="danger">
            Lecture impossible. Le format n’est peut-être pas lisible par la fenêtre (AVI, codec
            exotique) — le fichier, lui, est toujours là.
          </p>
        ) : (
          <video
            key={filePath}
            src={videoSrc(filePath)}
            autoPlay
            loop
            controls
            muted={muted}
            onVolumeChange={(e) => setMuted((e.target as HTMLVideoElement).muted)}
            onError={() => setFailed(true)}
          />
        )}

        {subtitle && <p className="mono small muted">{subtitle}</p>}
      </div>
    </dialog>
  );
}
