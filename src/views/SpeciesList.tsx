import { useCallback, useEffect, useMemo, useState } from 'react';

import { createSpecies, deleteSpecies, listSpecies, updateSpecies } from '../api';
import type { Species, SpeciesGroup, SpeciesInput } from '../api';
import { Confirm } from '../components/Confirm';

const GROUPS: { value: SpeciesGroup; label: string }[] = [
  { value: 'mammifere', label: 'Mammifères' },
  { value: 'oiseau', label: 'Oiseaux' },
  { value: 'autre', label: 'Autres' },
];

const EMPTY: SpeciesInput = {
  common_name: '',
  scientific_name: null,
  species_group: 'mammifere',
  color: '#8a7f6a',
  sort_order: 1000,
  shortcut_key: null,
};

export function SpeciesList({ onError }: { onError: (e: string | null) => void }) {
  const [species, setSpecies] = useState<Species[]>([]);
  const [editing, setEditing] = useState<{ id: string | null; input: SpeciesInput } | null>(null);
  const [confirming, setConfirming] = useState<Species | null>(null);
  const [query, setQuery] = useState('');

  const refresh = useCallback(async () => {
    try {
      setSpecies(await listSpecies());
    } catch (e) {
      onError(String(e));
    }
  }, [onError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const save = async () => {
    if (!editing) return;
    onError(null);
    try {
      if (editing.id) await updateSpecies(editing.id, editing.input);
      else await createSpecies(editing.input);
      setEditing(null);
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const remove = async (s: Species) => {
    onError(null);
    try {
      await deleteSpecies(s.id);
      setConfirming(null);
      await refresh();
    } catch (e) {
      setConfirming(null);
      onError(String(e));
    }
  };

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return species;
    return species.filter(
      (s) =>
        s.common_name.toLowerCase().includes(q) ||
        (s.scientific_name ?? '').toLowerCase().includes(q),
    );
  }, [species, query]);

  const takenKeys = useMemo(
    () =>
      species
        .filter((s) => s.shortcut_key && s.id !== editing?.id)
        .map((s) => s.shortcut_key as string),
    [species, editing?.id],
  );

  return (
    <div className="stack">
      <section className="panel stack">
        <div className="row row--flush">
          <h2>Espèces</h2>
          <span className="app__spacer" />
          <input
            className="search"
            placeholder="Rechercher…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button
            className="primary"
            onClick={() => setEditing({ id: null, input: { ...EMPTY } })}
          >
            Nouvelle espèce
          </button>
        </div>

        <p className="muted small">
          Les raccourcis servent au dépouillement au clavier. <code>0</code>, <code>1</code>,{' '}
          <code>2</code> et <code>3</code> sont réservés (écarter, et les trois niveaux de
          confiance).
        </p>

        {GROUPS.map((g) => {
          const rows = filtered.filter((s) => s.species_group === g.value);
          if (!rows.length) return null;
          return (
            <div key={g.value} className="stack">
              <h3>{g.label}</h3>
              <table className="table">
                <thead>
                  <tr>
                    <th />
                    <th>Nom</th>
                    <th>Nom scientifique</th>
                    <th>Raccourci</th>
                    <th className="num">Séquences</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {rows.map((s) => (
                    <tr key={s.id}>
                      <td>
                        <span
                          className="swatch"
                          style={{ background: s.color ?? 'transparent' }}
                          aria-hidden
                        />
                      </td>
                      <td>{s.common_name}</td>
                      <td className="muted">
                        <em>{s.scientific_name ?? '—'}</em>
                      </td>
                      <td>{s.shortcut_key ? <kbd>{s.shortcut_key}</kbd> : <span className="muted">—</span>}</td>
                      <td className="num">{s.usage_count}</td>
                      <td className="row--actions">
                        <button onClick={() => setEditing({ id: s.id, input: toInput(s) })}>
                          Modifier
                        </button>
                        <button onClick={() => setConfirming(s)} disabled={s.usage_count > 0}>
                          Supprimer
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        })}
      </section>

      {editing && (
        <SpeciesForm
          value={editing.input}
          isNew={!editing.id}
          takenKeys={takenKeys}
          onChange={(input) => setEditing({ ...editing, input })}
          onCancel={() => setEditing(null)}
          onSave={save}
        />
      )}

      {confirming && (
        <Confirm
          title={`Supprimer « ${confirming.common_name} » ?`}
          body="Aucune séquence ne porte cette espèce."
          confirmLabel="Supprimer"
          onConfirm={() => remove(confirming)}
          onCancel={() => setConfirming(null)}
        />
      )}
    </div>
  );
}

function toInput(s: Species): SpeciesInput {
  return {
    common_name: s.common_name,
    scientific_name: s.scientific_name,
    species_group: s.species_group,
    color: s.color,
    sort_order: s.sort_order,
    shortcut_key: s.shortcut_key,
  };
}

function SpeciesForm({
  value,
  isNew,
  takenKeys,
  onChange,
  onCancel,
  onSave,
}: {
  value: SpeciesInput;
  isNew: boolean;
  takenKeys: string[];
  onChange: (v: SpeciesInput) => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  const set = <K extends keyof SpeciesInput>(key: K, v: SpeciesInput[K]) =>
    onChange({ ...value, [key]: v });

  const key = value.shortcut_key?.toLowerCase() ?? '';
  // Signalé à la saisie plutôt qu'au rejet : corriger avant d'enregistrer coûte moins
  // cher que comprendre un message d'erreur après coup.
  const keyTaken = key !== '' && takenKeys.includes(key);
  const keyReserved = ['0', '1', '2', '3'].includes(key);

  return (
    <section className="panel stack">
      <h2>{isNew ? 'Nouvelle espèce' : `Modifier « ${value.common_name} »`}</h2>
      <div className="fields">
        <label>
          Nom courant
          <input
            value={value.common_name}
            onChange={(e) => set('common_name', e.target.value)}
            autoFocus
          />
        </label>
        <label>
          Nom scientifique
          <input
            value={value.scientific_name ?? ''}
            onChange={(e) => set('scientific_name', e.target.value || null)}
            placeholder="Lynx lynx"
          />
        </label>
        <label>
          Groupe
          <select
            value={value.species_group}
            onChange={(e) => set('species_group', e.target.value as SpeciesGroup)}
          >
            {GROUPS.map((g) => (
              <option key={g.value} value={g.value}>
                {g.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Couleur
          <input
            type="color"
            value={value.color ?? '#8a7f6a'}
            onChange={(e) => set('color', e.target.value)}
          />
        </label>
        <label>
          Raccourci
          <input
            value={value.shortcut_key ?? ''}
            maxLength={1}
            onChange={(e) => set('shortcut_key', e.target.value || null)}
          />
        </label>
        <label>
          Ordre
          <input
            value={value.sort_order}
            onChange={(e) => set('sort_order', Number(e.target.value) || 1000)}
          />
        </label>
      </div>

      {keyTaken && <p className="danger small">Ce raccourci est déjà pris par une autre espèce.</p>}
      {keyReserved && (
        <p className="danger small">Cette touche est réservée au dépouillement.</p>
      )}

      <div className="row row--flush">
        <button
          className="primary"
          onClick={onSave}
          disabled={!value.common_name.trim() || keyTaken || keyReserved}
        >
          Enregistrer
        </button>
        <button onClick={onCancel}>Annuler</button>
      </div>
    </section>
  );
}
