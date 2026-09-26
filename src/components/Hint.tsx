import { useRef, useState } from 'react';

/**
 * Une bulle « ? » : l'explication n'apparaît qu'au survol (ou au focus clavier).
 * Placée en `position: fixed` d'après la bulle elle-même, pour ne pas être rognée par un
 * panneau qui défile — celui des filtres, notamment.
 */
export function Hint({ children }: { children: React.ReactNode }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [at, setAt] = useState<{ top: number; left: number } | null>(null);

  const show = () => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    // 300 px : la largeur de la bulle. On la garde dans la fenêtre.
    setAt({ top: r.bottom + 6, left: Math.max(8, Math.min(r.left - 12, window.innerWidth - 308)) });
  };

  return (
    <span
      ref={ref}
      className="hint"
      tabIndex={0}
      aria-label="Explication"
      onMouseEnter={show}
      onMouseLeave={() => setAt(null)}
      onFocus={show}
      onBlur={() => setAt(null)}
    >
      ?
      {at && (
        <span className="hint__bubble" role="tooltip" style={at}>
          {children}
        </span>
      )}
    </span>
  );
}
