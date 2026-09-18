import { useEffect, useMemo, useRef, useState } from 'react';

import { CONFIDENCES } from '../api';
import type { Confidence, Species } from '../api';

/** Enlève les accents pour que « chevreuil » trouve « Chevreuil » et « ecureuil » « Écureuil ». */
function fold(value: string): string {
  return value
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();
}

/**
 * Liste déroulante recherchable des espèces.
 *
 * Toutes les espèces y sont : se limiter à celles qui ont un raccourci rendait le
 * référentiel inutilisable dès qu'on sortait des quelques habituées.
 *
 * `onSearchFocus` prévient le parent : pendant la saisie, les raccourcis globaux du
 * mode plein écran doivent se taire, sans quoi taper « sanglier » déclencherait une
 * demi-douzaine d'actions.
 */
export function SpeciesPicker({
  species,
  selected,
  onToggle,
  onSetConfidence,
  onSearchFocus,
  autoFocus = false,
}: {
  species: Species[];
  /** Espèces retenues, avec leur confiance. */
  selected: Map<string, Confidence>;
  onToggle: (id: string) => void;
  onSetConfidence?: (id: string, confidence: Confidence) => void;
  onSearchFocus?: (focused: boolean) => void;
  autoFocus?: boolean;
}) {
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const matches = useMemo(() => {
    const q = fold(query.trim());
    if (!q) return species;
    return species.filter(
      (s) =>
        fold(s.common_name).includes(q) ||
        fold(s.scientific_name ?? '').includes(q) ||
        s.shortcut_key === q,
    );
  }, [species, query]);

  useEffect(() => {
    setHighlight(0);
  }, [query]);

  // L'élément survolé au clavier doit rester visible : sur trente-sept espèces, la
  // sélection sort vite de la fenêtre.
  useEffect(() => {
    const el = listRef.current?.children[highlight] as HTMLElement | undefined;
    el?.scrollIntoView({ block: 'nearest' });
  }, [highlight]);

  const choose = (id: string) => {
    onToggle(id);
    setQuery('');
    inputRef.current?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Le champ capte tout : ce qu'on tape ici ne doit jamais atteindre les raccourcis.
    e.stopPropagation();
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setOpen(true);
      setHighlight((h) => Math.min(matches.length - 1, h + 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight((h) => Math.max(0, h - 1));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const pick = matches[highlight];
      if (pick) choose(pick.id);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (query) setQuery('');
      else inputRef.current?.blur();
    }
  };

  const chosen = species.filter((s) => selected.has(s.id));

  return (
    <div className="picker">
      <div className="picker__field">
        <input
          ref={inputRef}
          className="picker__input"
          placeholder="Chercher une espèce…"
          value={query}
          autoFocus={autoFocus}
          onChange={(e) => {
            setQuery(e.target.value);
            setOpen(true);
          }}
          onFocus={() => {
            setOpen(true);
            onSearchFocus?.(true);
          }}
          onBlur={() => {
            // Laisser le temps au clic sur un élément de la liste d'aboutir.
            window.setTimeout(() => setOpen(false), 150);
            onSearchFocus?.(false);
          }}
          onKeyDown={onKeyDown}
        />
        {open && (
          <ul className="picker__list" ref={listRef}>
            {matches.length === 0 && <li className="picker__empty">Aucune espèce</li>}
            {matches.map((s, i) => (
              <li
                key={s.id}
                className={[
                  'picker__item',
                  i === highlight ? 'picker__item--on' : '',
                  selected.has(s.id) ? 'picker__item--picked' : '',
                ]
                  .filter(Boolean)
                  .join(' ')}
                onMouseEnter={() => setHighlight(i)}
                onMouseDown={(e) => {
                  // `mousedown` et non `click` : le `blur` du champ fermerait la liste avant.
                  e.preventDefault();
                  choose(s.id);
                }}
              >
                <span className="swatch" style={{ background: s.color ?? 'transparent' }} />
                <span className="picker__name">{s.common_name}</span>
                {s.scientific_name && (
                  <em className="muted picker__latin">{s.scientific_name}</em>
                )}
                {s.shortcut_key && <kbd>{s.shortcut_key}</kbd>}
              </li>
            ))}
          </ul>
        )}
      </div>

      {chosen.length > 0 && (
        <div className="picker__chosen">
          {chosen.map((s) => {
            const confidence = selected.get(s.id) as Confidence;
            return (
              <span key={s.id} className={`chip chip--${confidence} chip--big`}>
                <span className="swatch" style={{ background: s.color ?? 'transparent' }} />
                {s.common_name}
                {onSetConfidence && (
                  <select
                    value={confidence}
                    onChange={(e) => onSetConfidence(s.id, e.target.value as Confidence)}
                    onKeyDown={(e) => e.stopPropagation()}
                    title="Confiance"
                  >
                    {CONFIDENCES.map((c) => (
                      <option key={c.value} value={c.value}>
                        {c.label}
                      </option>
                    ))}
                  </select>
                )}
                <button
                  className="chip__remove"
                  onClick={() => onToggle(s.id)}
                  title="Retirer"
                  aria-label={`Retirer ${s.common_name}`}
                >
                  ×
                </button>
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}
