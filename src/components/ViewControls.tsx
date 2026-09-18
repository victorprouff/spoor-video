import { useEffect, useState } from 'react';

export type Display = 'tiles' | 'list';

/**
 * Réglages d'affichage retenus d'une session à l'autre. Chaque vue a sa propre clé :
 * on ne regarde pas des fichiers comme on regarde des passages, et imposer le même
 * mode aux deux obligerait à rebasculer à chaque va-et-vient.
 */
export function useDisplay(key: string, fallback: Display = 'tiles') {
  const [display, setDisplay] = useState<Display>(() => {
    try {
      const stored = localStorage.getItem(`display-${key}`);
      return stored === 'tiles' || stored === 'list' ? stored : fallback;
    } catch {
      return fallback;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(`display-${key}`, display);
    } catch {
      /* sans conséquence : le choix tient pour la session */
    }
  }, [key, display]);

  return [display, setDisplay] as const;
}

const MIN = 120;
const MAX = 420;

export function useThumbSize(key: string, fallback = 220) {
  const [size, setSize] = useState<number>(() => {
    try {
      const stored = Number(localStorage.getItem(`thumb-${key}`));
      return Number.isFinite(stored) && stored >= MIN && stored <= MAX ? stored : fallback;
    } catch {
      return fallback;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(`thumb-${key}`, String(size));
    } catch {
      /* sans conséquence */
    }
  }, [key, size]);

  return [size, setSize] as const;
}

export function ViewControls({
  display,
  onDisplay,
  size,
  onSize,
  tilesLabel = 'Tuiles',
}: {
  display: Display;
  onDisplay: (d: Display) => void;
  size: number;
  onSize: (n: number) => void;
  tilesLabel?: string;
}) {
  return (
    <span className="viewctl">
      <button
        className={display === 'tiles' ? 'tab--on' : undefined}
        onClick={() => onDisplay('tiles')}
      >
        {tilesLabel}
      </button>
      <button
        className={display === 'list' ? 'tab--on' : undefined}
        onClick={() => onDisplay('list')}
      >
        Liste
      </button>
      <label className="inline" title="Taille des miniatures">
        <span aria-hidden>▫</span>
        <input
          type="range"
          min={MIN}
          max={MAX}
          step={20}
          value={size}
          onChange={(e) => onSize(Number(e.target.value))}
        />
        <span aria-hidden>◻</span>
      </label>
    </span>
  );
}
