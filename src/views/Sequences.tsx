import { useCallback, useEffect, useMemo, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';

import {
  listSequenceVideos,
  listSequences,
  listTraps,
  mergeSequences,
  regroupSequences,
  splitSequence,
} from '../api';
import type { RegroupReport, Sequence, SequenceVideo, Trap } from '../api';
import { formatDateTime } from '../format';

/** Une durée lisible : « 2 h 05 » dit tout de suite ce que « 7500 s » cache. */
function duration(seconds: number): string {
  if (seconds <= 0) return 'instantané';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h} h ${String(m).padStart(2, '0')}`;
  if (m > 0) return `${m} min ${String(s).padStart(2, '0')}`;
  return `${s} s`;
}

export function Sequences({ onError }: { onError: (e: string | null) => void }) {
  const [sequences, setSequences] = useState<Sequence[]>([]);
  const [traps, setTraps] = useState<Trap[]>([]);
  const [trapFilter, setTrapFilter] = useState<string>('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<string | null>(null);
  const [report, setReport] = useState<RegroupReport | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setSequences(await listSequences(trapFilter || null));
      setTraps(await listTraps());
    } catch (e) {
      onError(String(e));
    }
  }, [onError, trapFilter]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const regroup = async () => {
    setBusy(true);
    onError(null);
    try {
      setReport(await regroupSequences());
      setSelected(new Set());
      await refresh();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const merge = async () => {
    onError(null);
    try {
      await mergeSequences([...selected]);
      setSelected(new Set());
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const split = async (sequenceId: string, videoId: string) => {
    onError(null);
    try {
      await splitSequence(sequenceId, videoId);
      await refresh();
    } catch (e) {
      onError(String(e));
    }
  };

  const toggle = (id: string) => {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  };

  // Fusionner n'a de sens qu'entre séquences d'un même piège : on le dit avant
  // plutôt que de laisser le serveur refuser.
  const selectedSeqs = useMemo(
    () => sequences.filter((s) => selected.has(s.id)),
    [sequences, selected],
  );
  const sameTrap =
    selectedSeqs.length > 1 && new Set(selectedSeqs.map((s) => s.trap_id)).size === 1;

  return (
    <div className="stack">
      <section className="panel stack">
        <div className="row row--flush">
          <h2>Séquences</h2>
          <span className="app__spacer" />
          <select value={trapFilter} onChange={(e) => setTrapFilter(e.target.value)}>
            <option value="">Tous les pièges</option>
            {traps.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
          <button
            onClick={merge}
            disabled={!sameTrap}
            title={
              selectedSeqs.length > 1 && !sameTrap
                ? 'Ces séquences appartiennent à des pièges différents'
                : undefined
            }
          >
            Fusionner ({selectedSeqs.length})
          </button>
          <button className="primary" onClick={regroup} disabled={busy}>
            {busy ? 'Regroupement…' : 'Regrouper'}
          </button>
        </div>

        <p className="muted small">
          Une séquence regroupe les vidéos d’un même piège séparées de moins de dix minutes. Un
          regroupement ne défait jamais une séquence scindée, fusionnée ou déjà annotée.
        </p>

        {report && (
          <ul className="tallies">
            <li>
              <span className="tallies__n">{report.sequences_built}</span> reconstruites
            </li>
            <li className={report.sequences_frozen ? undefined : 'muted'}>
              <span className="tallies__n">{report.sequences_frozen}</span> intactes
            </li>
            <li>
              <span className="tallies__n">{report.videos_grouped}</span> vidéos groupées
            </li>
            <li className={report.videos_undated ? undefined : 'muted'}>
              <span className="tallies__n">{report.videos_undated}</span> sans date
            </li>
          </ul>
        )}

        {report != null && report.videos_undated > 0 && (
          <p className="muted small">
            Sans date exploitable, une vidéo n’a pas de place dans une chronologie : elle reste
            hors séquence plutôt que d’être rangée à un endroit inventé.
          </p>
        )}

        {sequences.length === 0 ? (
          <p className="muted">
            Aucune séquence. Lance une passe d’indexation, ou regroupe si des vidéos sont déjà
            indexées.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th />
                <th>Début</th>
                <th>Piège</th>
                <th className="num">Vidéos</th>
                <th>Durée</th>
                <th>État</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {sequences.map((s) => (
                <SequenceRow
                  key={s.id}
                  sequence={s}
                  checked={selected.has(s.id)}
                  expanded={expanded === s.id}
                  onToggleCheck={() => toggle(s.id)}
                  onToggleExpand={() => setExpanded(expanded === s.id ? null : s.id)}
                  onSplit={(videoId) => split(s.id, videoId)}
                  onError={onError}
                />
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}

function SequenceRow({
  sequence,
  checked,
  expanded,
  onToggleCheck,
  onToggleExpand,
  onSplit,
  onError,
}: {
  sequence: Sequence;
  checked: boolean;
  expanded: boolean;
  onToggleCheck: () => void;
  onToggleExpand: () => void;
  onSplit: (videoId: string) => void;
  onError: (e: string | null) => void;
}) {
  const [videos, setVideos] = useState<SequenceVideo[] | null>(null);

  useEffect(() => {
    if (!expanded || videos) return;
    listSequenceVideos(sequence.id)
      .then(setVideos)
      .catch((e) => onError(String(e)));
  }, [expanded, videos, sequence.id, onError]);

  return (
    <>
      <tr>
        <td>
          <input type="checkbox" checked={checked} onChange={onToggleCheck} />
        </td>
        <td>{formatDateTime(sequence.started_at)}</td>
        <td>{sequence.trap_name}</td>
        <td className="num">{sequence.video_count}</td>
        <td>
          {duration(sequence.duration_s)}
          {sequence.duration_s >= 3600 && (
            <span className="muted small"> · activité continue</span>
          )}
        </td>
        <td>
          {!sequence.auto_grouped && (
            <span className="badge" title="Découpage manuel : un regroupement ne le défera pas">
              manuel
            </span>
          )}
          {sequence.reviewed_at ? (
            <span className="badge badge--ok">dépouillée</span>
          ) : (
            <span className="muted small">à dépouiller</span>
          )}
        </td>
        <td className="row--actions">
          <button onClick={onToggleExpand}>{expanded ? 'Replier' : 'Vidéos'}</button>
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={7}>
            {videos === null ? (
              <p className="muted">Chargement…</p>
            ) : (
              <div className="thumbs">
                {videos.map((v, i) => (
                  <figure key={v.id} className="thumb">
                    {v.thumbnail_path ? (
                      <img src={convertFileSrc(v.thumbnail_path)} alt="" loading="lazy" />
                    ) : (
                      <div className="thumb__none">pas de vignette</div>
                    )}
                    <figcaption>
                      <span className="mono small">{v.file_name}</span>
                      <span className="muted small">{formatDateTime(v.recorded_at)}</span>
                      {v.file_state !== 'present' && (
                        <span className="badge badge--warn">
                          {v.file_state === 'missing' ? 'disparue' : 'supprimée'}
                        </span>
                      )}
                      {i > 0 && (
                        <button
                          className="small"
                          onClick={() => onSplit(v.id)}
                          title="Cette vidéo et les suivantes partent dans une nouvelle séquence"
                        >
                          Scinder ici
                        </button>
                      )}
                    </figcaption>
                  </figure>
                ))}
              </div>
            )}
          </td>
        </tr>
      )}
    </>
  );
}
