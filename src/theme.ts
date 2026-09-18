import { useCallback, useEffect, useState } from 'react';

export type Theme = 'light' | 'dark';

/** Thème par défaut : celui du système, tant que rien n'a été choisi explicitement. */
function systemTheme(): Theme {
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function stored(): Theme | null {
  try {
    const t = localStorage.getItem('theme');
    return t === 'light' || t === 'dark' ? t : null;
  } catch {
    // Stockage inaccessible : on se rabat sur le système, sans casser le rendu.
    return null;
  }
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() => stored() ?? systemTheme());

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
    try {
      localStorage.setItem('theme', theme);
    } catch {
      /* sans conséquence : le thème tient pour la session */
    }
  }, [theme]);

  const toggle = useCallback(() => {
    setTheme((t) => (t === 'dark' ? 'light' : 'dark'));
  }, []);

  return { theme, toggle };
}
