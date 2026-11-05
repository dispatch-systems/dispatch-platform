import { useState, type ReactNode } from 'react';
import { Modal } from './Modal.js';

/** Confirms what can't be undone once `expected` is typed, exactly: `onConfirm` gets it as typed. */
export function TypedConfirmDialog({
  title,
  label,
  expected,
  confirm,
  busy = false,
  onConfirm,
  onCancel,
}: {
  title: ReactNode;
  label: string;
  expected: string;
  confirm: string;
  busy?: boolean;
  onConfirm: (typed: string) => void;
  onCancel: () => void;
}) {
  const [typed, setTyped] = useState('');
  return (
    <Modal title={title} onClose={onCancel}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (typed === expected) onConfirm(typed);
        }}
      >
        <label>
          {label}
          <input
            value={typed}
            placeholder={expected}
            autoComplete="off"
            spellCheck={false}
            disabled={busy}
            onChange={(event) => setTyped(event.target.value)}
          />
        </label>
        <div className="form-actions">
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button className="danger" disabled={busy || typed !== expected}>
            {confirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}
