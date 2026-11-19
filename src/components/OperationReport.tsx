import { useModalFocus } from './useModalFocus';
import { m1 } from '../app/m1Strings';

export function OperationReport({
  value,
  onClose,
}: {
  value: unknown;
  onClose: () => void;
}) {
  const modal = useModalFocus();
  const fields =
    value && typeof value === 'object' ? Object.entries(value) : [];
  const labels: Record<string, string> = {
    path: m1.destination,
    images: m1.images,
    annotations: m1.annotations,
    imported: m1.imported,
    duplicates: m1.duplicates,
    cancelled: m1.cancelled,
    warnings: m1.warnings,
  };
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        role="dialog"
        aria-modal="true"
        aria-label={m1.report}
        className="dialog"
      >
        <h2>{m1.report}</h2>
        <dl className="operation-report">
          {fields.map(([key, entry]) => (
            <div key={key}>
              <dt>{labels[key] ?? key}</dt>
              <dd>
                {Array.isArray(entry) ? (
                  entry.length ? (
                    <ul>
                      {entry.map((item, i) => (
                        <li key={i}>{String(item)}</li>
                      ))}
                    </ul>
                  ) : (
                    m1.none
                  )
                ) : typeof entry === 'boolean' ? (
                  entry ? (
                    m1.yes
                  ) : (
                    m1.no
                  )
                ) : (
                  String(entry)
                )}
              </dd>
            </div>
          ))}
        </dl>
        <div className="dialog-actions">
          <button onClick={onClose}>{m1.close}</button>
        </div>
      </section>
    </div>
  );
}
