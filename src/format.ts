/**
 * SQLite rend ses dates en `YYYY-MM-DD HH:MM:SS` (sans fuseau) là où l'indexation
 * stocke de l'ISO-8601. `new Date()` interprète le premier comme de l'heure locale
 * sur certains moteurs et pas sur d'autres : on normalise avant d'afficher, sans quoi
 * une même date s'afficherait décalée d'une vue à l'autre.
 */
function parse(raw: string): Date | null {
  const normalised = raw.includes('T') ? raw : `${raw.replace(' ', 'T')}Z`;
  const d = new Date(normalised);
  return Number.isNaN(d.getTime()) ? null : d;
}

export function formatDate(raw: string | null | undefined): string {
  if (!raw) return '—';
  const d = parse(raw);
  return d ? d.toLocaleDateString('fr-FR') : '—';
}

export function formatDateTime(raw: string | null | undefined): string {
  if (!raw) return '—';
  const d = parse(raw);
  return d ? d.toLocaleString('fr-FR') : '—';
}
