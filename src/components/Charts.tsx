/**
 * Graphiques en SVG maison, sans bibliothèque — comme le CSS de Spoor.
 * Les couleurs viennent des variables de thème, donc tout suit le mode sombre.
 */

type Bar = {
  label: string;
  value: number;
  /** Sous-titre affiché sous la barre, pour une information de second rang. */
  hint?: string;
  color?: string | null;
  /** Barre atténuée : présente, mais pas au premier plan. */
  dim?: boolean;
};

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

  return (
    <div className="chart" style={{ ['--chart-h' as string]: `${height}px` }}>
      <div className="chart__bars">
        {bars.map((b, i) => (
          <div key={`${b.label}-${i}`} className={b.dim ? 'chart__col chart__col--dim' : 'chart__col'}>
            <span className="chart__value">{b.value > 0 ? format(b.value) : ''}</span>
            <div
              className="chart__bar"
              style={{
                height: `${(b.value / max) * 100}%`,
                background: b.color ?? 'var(--accent)',
              }}
              title={`${b.label} — ${format(b.value)}`}
            />
            <span className="chart__label">{b.label}</span>
            {b.hint && <span className="chart__hint">{b.hint}</span>}
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Plusieurs séries sur le même axe horaire, une ligne par espèce.
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
