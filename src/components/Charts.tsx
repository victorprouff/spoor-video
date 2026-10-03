import { useState } from 'react';

/**
 * Graphiques en CSS maison, sans bibliothèque — comme le reste des styles.
 * Les couleurs viennent des variables de thème, donc tout suit le mode sombre.
 */

type Bar = {
  label: string;
  value: number;
  /** Sous-titre affiché sous l'étiquette, pour une information de second rang. */
  hint?: string;
  color?: string | null;
  /** Barre atténuée : présente, mais pas au premier plan. */
  dim?: boolean;
  /** Libellé au survol, quand l'étiquette d'axe est laissée vide pour aérer. */
  title?: string;
};

/**
 * L'aire des barres et l'axe sont **deux grilles distinctes**, de même nombre de
 * colonnes, empilées.
 *
 * Une première version plaçait l'étiquette *dans* la colonne, sous la barre. La barre
 * la plus haute faisant 100 % de la colonne, elle débordait sur l'étiquette : les
 * graduations se retrouvaient au niveau des barres, et le graphique devenait illisible.
 * Séparer les deux garantit que rien ne se chevauche, quelle que soit la hauteur.
 */
export function BarChart({
  bars,
  height = 160,
  format = (n: number) => String(n),
  unit,
}: {
  bars: Bar[];
  height?: number;
  format?: (n: number) => string;
  /**
   * Active la bulle de survol, qui donne le nombre sous la souris dans cette unité
   * (« passage » → « 3 passages »). Sans elle, seul le `title` natif reste, lent à
   * venir et pas toujours affiché par la fenêtre de l'application.
   */
  unit?: string;
}) {
  const [hover, setHover] = useState<number | null>(null);

  if (bars.length === 0) {
    return <p className="muted small">Aucune donnée dans cette sélection.</p>;
  }
  // L'échelle part toujours de zéro : un axe tronqué exagère les écarts.
  const max = Math.max(...bars.map((b) => b.value), 1);
  // Au-delà d'une vingtaine de barres, les valeurs se chevauchent et ne servent plus
  // à rien : l'info reste au survol.
  const showValues = bars.length <= 20;
  // L'espace entre barres se resserre avec leur nombre : à 360 barres, 2 px d'écart
  // mangeraient la place des barres elles-mêmes. Même écart pour l'axe, sinon les
  // étiquettes glissent par rapport à leurs barres.
  const gap = bars.length > 240 ? 0 : bars.length > 40 ? 1 : 2;
  const columns = { gridTemplateColumns: `repeat(${bars.length}, 1fr)`, gap };

  const hovered = unit && hover !== null ? bars[hover] : null;

  return (
    <div className="chart">
      <div
        className="chart__plot"
        style={{ ...columns, height }}
        onMouseLeave={() => setHover(null)}
      >
        {hovered && hover !== null && (
          <div
            className="chart__tip"
            style={{
              left: `${((hover + 0.5) / bars.length) * 100}%`,
              // Glisse de 0 à -100 % selon la position : la bulle reste dans le graphique
              // au lieu de déborder sur les bords.
              transform: `translateX(-${(hover / Math.max(bars.length - 1, 1)) * 100}%)`,
            }}
          >
            <span className="muted">{hovered.title ?? hovered.label}</span>
            <span className="muted"> · </span>
            <strong>{format(hovered.value)}</strong> {unit}
            {hovered.value > 1 ? 's' : ''}
          </div>
        )}
        {bars.map((b, i) => (
          <div
            key={`${b.label}-${i}`}
            className={[
              'chart__col',
              b.dim && 'chart__col--dim',
              unit && hover === i && 'chart__col--on',
            ]
              .filter(Boolean)
              .join(' ')}
            title={unit ? undefined : `${b.title ?? (b.label || '—')} : ${format(b.value)}`}
            onMouseEnter={unit ? () => setHover(i) : undefined}
          >
            {showValues && b.value > 0 && <span className="chart__value">{format(b.value)}</span>}
            <div
              className="chart__bar"
              style={{
                height: `${(b.value / max) * 100}%`,
                background: b.color ?? 'var(--accent)',
              }}
            />
          </div>
        ))}
      </div>

      <div className="chart__axis" style={columns}>
        {bars.map((b, i) => (
          <span key={`axis-${b.label}-${i}`} className="chart__tick">
            <span className="chart__label">{b.label}</span>
            {b.hint && <span className="chart__hint">{b.hint}</span>}
          </span>
        ))}
      </div>
    </div>
  );
}

/**
 * Plusieurs séries sur le même axe, une ligne par espèce.
 *
 * Chaque série est normalisée sur son propre maximum : on compare des **formes**
 * d'activité, pas des abondances. Sinon l'espèce la plus fréquente écraserait toutes
 * les autres et on ne verrait plus aucun rythme.
 *
 * L'intensité d'une case ne se lit pas au jugé : survoler affiche l'espèce, l'heure et
 * le nombre de passages, et les graduations, discrètes au repos, se déplient toutes.
 */
export function SmallMultiples({
  series,
  slots,
  labelOf,
  fullLabelOf = (slot: number) => String(slot).padStart(2, '0'),
  unit = 'passage',
}: {
  series: { key: string; name: string; color?: string | null; values: Map<number, number> }[];
  slots: number[];
  /** Étiquette au repos — laisser vide pour n'en montrer qu'une sur six. */
  labelOf: (slot: number) => string;
  /** Étiquette complète, affichée au survol. */
  fullLabelOf?: (slot: number) => string;
  unit?: string;
}) {
  const [hover, setHover] = useState<{ key: string; slot: number; value: number } | null>(null);
  const [axisOpen, setAxisOpen] = useState(false);

  if (series.length === 0) {
    return <p className="muted small">Aucune espèce identifiée dans cette sélection.</p>;
  }

  const expanded = axisOpen || hover !== null;

  return (
    <div className="multiples" onMouseLeave={() => setHover(null)}>
      <div className="multiples__readout">
        {hover ? (
          <>
            <strong>{series.find((s) => s.key === hover.key)?.name}</strong>
            <span className="muted"> · </span>
            {fullLabelOf(hover.slot)}
            <span className="muted"> · </span>
            <strong>{hover.value}</strong> {unit}
            {hover.value > 1 ? 's' : ''}
          </>
        ) : (
          <span className="muted">Survole une case pour voir l’heure et le nombre de passages.</span>
        )}
      </div>

      {series.map((s) => {
        const max = Math.max(...slots.map((h) => s.values.get(h) ?? 0), 1);
        const total = slots.reduce((sum, h) => sum + (s.values.get(h) ?? 0), 0);
        return (
          <div key={s.key} className="multiples__row">
            <div className="multiples__name">
              <span className="swatch" style={{ background: s.color ?? 'var(--accent)' }} />
              {s.name}
              <span className="muted small"> · {total}</span>
            </div>
            <div className="multiples__track">
              {slots.map((slot) => {
                const v = s.values.get(slot) ?? 0;
                const active = hover?.slot === slot;
                return (
                  <span
                    key={slot}
                    className={active ? 'multiples__cell multiples__cell--on' : 'multiples__cell'}
                    onMouseEnter={() => setHover({ key: s.key, slot, value: v })}
                    style={{
                      // L'opacité dit l'intensité ; la case vide reste visible pour que
                      // l'axe se lise même là où il ne s'est rien passé.
                      background: s.color ?? 'var(--accent)',
                      opacity: v === 0 ? 0.07 : 0.25 + 0.75 * (v / max),
                    }}
                  />
                );
              })}
            </div>
          </div>
        );
      })}

      <div
        className="multiples__row multiples__axis"
        onMouseEnter={() => setAxisOpen(true)}
        onMouseLeave={() => setAxisOpen(false)}
      >
        <div className="multiples__name" />
        <div className="multiples__track">
          {slots.map((slot) => (
            <span
              key={slot}
              className={
                hover?.slot === slot ? 'multiples__tick multiples__tick--on' : 'multiples__tick'
              }
            >
              {/* Au repos une graduation sur six suffit ; au survol, toutes. */}
              {expanded ? fullLabelOf(slot) : labelOf(slot)}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
