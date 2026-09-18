/**
 * Les dates de capture sont de **l'heure murale au piège** : celle qu'affiche
 * l'horloge de la caméra, corrigée du décalage réglé sur le piège.
 *
 * Elles ne sont donc **jamais converties dans le fuseau de la machine**. Une séquence
 * enregistrée à 21 h doit s'afficher 21 h, quel que soit l'endroit d'où on la relit :
 * passer par `new Date(...)` la décalerait d'une ou deux heures selon la saison, et
 * défairait silencieusement le réglage du piège.
 *
 * Pour une caméra posée en forêt, c'est l'heure de la caméra qui fait foi — pas l'UTC,
 * qui ne veut rien dire pour l'animal qui passe.
 */

type Parts = {
  year: string;
  month: string;
  day: string;
  hour: string;
  minute: string;
  second: string;
};

/**
 * Découpe une date sans jamais l'interpréter. Accepte les deux formes produites par
 * l'application : l'ISO-8601 de l'indexation (`2026-03-14T21:00:00Z`) et le format que
 * rend SQLite après un calcul (`2026-03-14 21:00:00`).
 */
function parts(raw: string): Parts | null {
  const m = raw
    .trim()
    .match(/^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})(?::(\d{2}))?/);
  if (!m) return null;
  return {
    year: m[1],
    month: m[2],
    day: m[3],
    hour: m[4],
    minute: m[5],
    second: m[6] ?? '00',
  };
}

export function formatDate(raw: string | null | undefined): string {
  if (!raw) return '—';
  const p = parts(raw);
  return p ? `${p.day}/${p.month}/${p.year}` : '—';
}

export function formatDateTime(raw: string | null | undefined): string {
  if (!raw) return '—';
  const p = parts(raw);
  return p ? `${p.day}/${p.month}/${p.year} ${p.hour}:${p.minute}` : '—';
}

export function formatTime(raw: string | null | undefined): string {
  if (!raw) return '—';
  const p = parts(raw);
  return p ? `${p.hour}:${p.minute}` : '—';
}

/** L'heure, en nombre — pour les graphiques de rythme d'activité. */
export function hourOf(raw: string | null | undefined): number | null {
  if (!raw) return null;
  const p = parts(raw);
  return p ? Number(p.hour) : null;
}

const MONTHS = [
  'janvier',
  'février',
  'mars',
  'avril',
  'mai',
  'juin',
  'juillet',
  'août',
  'septembre',
  'octobre',
  'novembre',
  'décembre',
];

export function monthName(month: number): string {
  return MONTHS[month - 1] ?? String(month);
}

/** Une durée lisible : « 2 h 05 » dit tout de suite ce que « 7500 s » cache. */
export function formatDuration(seconds: number): string {
  if (seconds <= 0) return 'instantané';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0) return `${h} h ${String(m).padStart(2, '0')}`;
  if (m > 0) return `${m} min`;
  return `${seconds} s`;
}
