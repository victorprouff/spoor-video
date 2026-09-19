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
}: {
  bars: Bar[];
  height?: number;
  format?: (n: number) => string;
}) {
  if (bars.length === 0) {
    return <p className="muted small">Aucune donnée dans cette sélection.</p>;
  }
  // L'échelle part toujours de zéro : un axe tronqué exagère les écarts.
  const max = Math.max(...bars.map((b) => b.value), 1);
  // Au-delà d'une vingtaine de barres, les valeurs se chevauchent et ne servent plus
  // à rien : l'info reste au survol.
  const showValues = bars.length <= 20;
  const columns = { gridTemplateColumns: `repeat(${bars.length}, 1fr)` };

  return (
    <div className="chart">
      <div className="chart__plot" style={{ ...columns, height }}>
        {bars.map((b, i) => (
          <div
            key={`${b.label}-${i}`}
            className={b.dim ? 'chart__col chart__col--dim' : 'chart__col'}
            title={`${b.label || '—'} : ${format(b.value)}`}
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
 * Chaque série est normalisée sur son propre maximum : on compare des **formes**
 * d'activité, pas des abondances. Sinon l'espèce la plus fréquente écraserait
 * toutes les autres et on ne verrait plus aucun rythme.
 */
export function SmallMultiples({
  series,
  slots,
  labelOf,
}: {
  series: { key: string; name: string; color?: string | null; values: Map<number, number> }[];
  slots: number[];
  labelOf: (slot: number) => string;
}) {
  if (series.length === 0) {
    return <p className="muted small">Aucune espèce identifiée dans cette sélection.</p>;
  }

  return (
    <div className="multiples">
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
                return (
                  <span
                    key={slot}
                    className="multiples__cell"
                    title={`${labelOf(slot)} — ${v}`}
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
      <div className="multiples__row multiples__axis">
        <div className="multiples__name" />
        <div className="multiples__track">
          {slots.map((slot) => (
            <span key={slot} className="multiples__tick">
              {labelOf(slot)}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
