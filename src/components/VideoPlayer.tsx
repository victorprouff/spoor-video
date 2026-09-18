import { useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

/** Le même réglage de son que le mode plein écran : un seul choix, partout. */
function storedMuted(): boolean {
  try {
    return localStorage.getItem('video-muted') === '1';
  } catch {
    return false;
  }
}

export function VideoPlayer({
  filePath,
  title,
  subtitle,
  onClose,
}: {
  filePath: string;
  title: string;
  subtitle?: string;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [muted, setMuted] = useState(storedMuted);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem('video-muted', muted ? '1' : '0');
    } catch {
      /* sans conséquence : le choix tient pour la session */
    }
  }, [muted]);

  return (
    <dialog ref={ref} className="dialog dialog--player" onCancel={onClose} onClose={onClose}>
      <div className="row row--flush">
        <strong>{title}</strong>
        <span className="app__spacer" />
        <button onClick={() => setMuted((m) => !m)}>{muted ? 'Son coupé' : 'Son actif'}</button>
        <button onClick={onClose}>Fermer</button>
      </div>

      {failed ? (
        <p className="danger">
          Lecture impossible. Le format n’est peut-être pas lisible par la fenêtre (AVI, codec
          exotique) — le fichier, lui, est toujours là.
        </p>
      ) : (
        <video
          src={convertFileSrc(filePath)}
          autoPlay
          loop
          controls
          muted={muted}
          onVolumeChange={(e) => setMuted((e.target as HTMLVideoElement).muted)}
          onError={() => setFailed(true)}
        />
      )}

      {subtitle && <p className="mono small muted">{subtitle}</p>}
    </dialog>
  );
}
