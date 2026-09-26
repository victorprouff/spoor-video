import { useEffect, useRef } from 'react';

/**
 * Boîte de dialogue sur `<dialog>` natif, jamais `window.confirm` — convention Spoor.
 * Le focus va sur « Annuler », jamais sur l'action destructive : un `Entrée` réflexe
 * ne doit pas supprimer.
 */
export function Confirm({
  title,
  body,
  confirmLabel,
  destructive = true,
  onConfirm,
  onCancel,
}: {
  title: string;
  body?: string;
  confirmLabel: string;
  destructive?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    ref.current?.showModal();
    cancelRef.current?.focus();
  }, []);

  return (
    <dialog ref={ref} className="dialog" onCancel={onCancel} onClose={onCancel}>
      <h2>{title}</h2>
      {body && <p className="muted">{body}</p>}
      <div className="row row--flush">
        <button ref={cancelRef} onClick={onCancel}>
          Annuler
        </button>
        <span className="app__spacer" />
        <button className={destructive ? 'destructive' : 'primary'} onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </dialog>
  );
}
