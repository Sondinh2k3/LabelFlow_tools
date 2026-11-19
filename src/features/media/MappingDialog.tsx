import { useModalFocus } from '../../components/useModalFocus';
import { useState } from 'react';
import type {
  MaskFolderInspection,
  MaskInspection,
  Profile,
  YoloInspection,
} from '../../lib/contracts';
import { m1 } from '../../app/m1Strings';
import { prelabel } from '../../app/teamStrings';
type Mode = 'fillUnlabeled' | 'replace';
export function MappingDialog({
  inspection,
  profile,
  layerId,
  onClose,
  onApply,
  folder,
}: {
  inspection: YoloInspection | MaskInspection | MaskFolderInspection;
  profile: Profile;
  layerId: string;
  onClose: () => void;
  onApply: (mapping: { source: number; target: string }[], mode: Mode) => void;
  folder?: MaskFolderInspection;
}) {
  const modal = useModalFocus();
  const mask = 'values' in inspection;
  const values = mask ? inspection.values : inspection.classes;
  const layer = profile.layers.find((l) => l.id === layerId);
  const candidates = mask
    ? layer?.layerType === 'region'
      ? [
          { id: '0', displayName: '0 — background' },
          { id: '1', displayName: '1 — region' },
        ]
      : profile.classes
          .filter((c) => c.namespace === layer?.namespace)
          .map((c) => ({
            id: String(c.exportId),
            displayName: `${c.exportId} — ${c.displayName}`,
          }))
    : profile.classes
        .filter((c) => c.namespace === 'candidate' && !c.isIgnore)
        .map((c) => ({
          id: c.id,
          displayName: `${c.exportId} — ${c.displayName}`,
        }));
  // Masks already written in project IDs map to themselves; anything else stays unmapped.
  const [mapping, setMapping] = useState<Record<number, string>>(() =>
    mask
      ? Object.fromEntries(
          values
            .filter((v) => candidates.some((c) => c.id === String(v)))
            .map((v) => [v, String(v)]),
        )
      : {},
  );
  const [mode, setMode] = useState<Mode>('fillUnlabeled');
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={m1.mapping}
      >
        <h2>{m1.mapping}</h2>
        <p>{mask ? m1.maskHelp : m1.mappingHelp}</p>
        {folder && (
          <>
            <p>{prelabel.matched(folder.matched, folder.approvedSkipped)}</p>
            {folder.warnings.length > 0 && (
              <details>
                <summary>{prelabel.warnings(folder.warnings.length)}</summary>
                <ul className="mapping-warnings">
                  {folder.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              </details>
            )}
            <label className="mapping-row">
              <span>{prelabel.mode}</span>
              <select
                aria-label={prelabel.mode}
                value={mode}
                onChange={(e) =>
                  setMode(
                    e.target.value === 'replace' ? 'replace' : 'fillUnlabeled',
                  )
                }
              >
                <option value="fillUnlabeled">{prelabel.fillUnlabeled}</option>
                <option value="replace">{prelabel.replace}</option>
              </select>
            </label>
          </>
        )}
        <div className="mapping-list">
          {values.map((source) => (
            <label className="mapping-row" key={source}>
              <span>
                {m1.source} {source}
                {!mask && inspection.names?.[source]
                  ? ` · ${inspection.names[source]}`
                  : ''}
              </span>
              <select
                aria-label={`${m1.source} ${source}`}
                value={mapping[source] ?? ''}
                onChange={(e) =>
                  setMapping({ ...mapping, [source]: e.target.value })
                }
              >
                <option value="">{m1.unmapped}</option>
                {candidates.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.displayName}
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
        <div className="dialog-actions">
          <button onClick={onClose}>{m1.close}</button>
          <button
            className="primary"
            disabled={values.some((v) => !mapping[v])}
            onClick={() =>
              onApply(
                values.map((source) => ({ source, target: mapping[source]! })),
                mode,
              )
            }
          >
            {folder ? prelabel.apply : mask ? m1.applyMask : m1.applyLabels}
          </button>
        </div>
      </section>
    </div>
  );
}
