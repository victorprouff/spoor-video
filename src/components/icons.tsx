/**
 * Icônes du lecteur, dessinées sur la même grille que l'engrenage des Réglages :
 * 16 × 16, trait de 1,3, couleur héritée du texte (`currentColor`), donc justes dans
 * les deux thèmes sans rien de plus.
 */
function Svg({ children, filled }: { children: React.ReactNode; filled?: boolean }) {
  return (
    <svg
      className="icon"
      viewBox="0 0 16 16"
      fill={filled ? 'currentColor' : 'none'}
      stroke="currentColor"
      strokeWidth="1.3"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function PrevIcon() {
  return (
    <Svg>
      <path d="M10 3 L5 8 L10 13" />
    </Svg>
  );
}

export function NextIcon() {
  return (
    <Svg>
      <path d="M6 3 L11 8 L6 13" />
    </Svg>
  );
}

export function SoundIcon({ muted }: { muted: boolean }) {
  return (
    <Svg>
      <path d="M2.5 6 H5 L8.5 3 V13 L5 10 H2.5 Z" />
      {muted ? (
        <path d="M11 6 L14.5 9.5 M14.5 6 L11 9.5" />
      ) : (
        <path d="M11 5.5 Q12.6 8 11 10.5 M12.9 3.8 Q15.6 8 12.9 12.2" />
      )}
    </Svg>
  );
}

export function StarIcon({ filled }: { filled: boolean }) {
  return (
    <Svg filled={filled}>
      <path d="M8 2 L9.65 6.33 L14.28 6.56 L10.66 9.47 L11.88 13.94 L8 11.4 L4.12 13.94 L5.34 9.47 L1.72 6.56 L6.35 6.33 Z" />
    </Svg>
  );
}

/** Coins vers l'extérieur pour entrer en plein écran, vers l'intérieur pour en sortir. */
export function FullscreenIcon({ exit }: { exit: boolean }) {
  return (
    <Svg>
      {exit ? (
        <path d="M6 2 V6 H2 M10 2 V6 H14 M10 14 V10 H14 M6 14 V10 H2" />
      ) : (
        <path d="M2 6 V2 H6 M10 2 H14 V6 M14 10 V14 H10 M6 14 H2 V10" />
      )}
    </Svg>
  );
}

export function CloseIcon() {
  return (
    <Svg>
      <path d="M3.5 3.5 L12.5 12.5 M12.5 3.5 L3.5 12.5" />
    </Svg>
  );
}
