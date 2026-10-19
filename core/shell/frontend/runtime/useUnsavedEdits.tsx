import { useEffect, useRef, useState } from 'react';
import { Modal } from '../ui/Modal.js';
import { useLeaveGuard, type Leave } from './leave.js';

/**
 * An editor's unsaved edits, kept until the member saves or discards them, as the editor is left
 * through `leave` or the page it's on moves on. With none (`dirty` false), either goes on at once;
 * with some, `asking`, drawn while it asks, has the member Save or Discard them first, and Save
 * goes on once `save` has saved; Cancel, or Escape, stays. Closing or reloading the tab while
 * there are some has the browser ask.
 */
export function useUnsavedEdits({
  what,
  dirty,
  saveable,
  save,
}: {
  /** What has the edits, as the question names it: "This schedule". */
  what: string;
  dirty: boolean;
  /** Whether Save may be chosen: the edits may be saved as they are. */
  saveable: boolean;
  /** Saves the edits; whether they were. */
  save: () => Promise<boolean>;
}) {
  const [confirming, setConfirming] = useState<(() => void) | null>(null);
  const [saving, setSaving] = useState(false);
  const leave: Leave = (then) => (dirty ? setConfirming(() => then) : then());
  const guard = useRef<Leave | null>(null);
  useEffect(() => {
    guard.current = leave;
  });
  useLeaveGuard(guard);
  useEffect(() => {
    if (!dirty) return;
    const stay = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener('beforeunload', stay);
    return () => window.removeEventListener('beforeunload', stay);
  }, [dirty]);
  // Once Save is chosen, the answer is the save's.
  const stay = () => !saving && setConfirming(null);
  const asking = confirming && (
    <Modal title="Save changes?" onClose={stay} above>
      <p>{what} has unsaved changes.</p>
      <div className="form-actions">
        <button type="button" disabled={saving} onClick={stay}>
          Cancel
        </button>
        <button
          type="button"
          disabled={saving}
          onClick={() => {
            setConfirming(null);
            confirming();
          }}
        >
          Discard changes
        </button>
        <button
          type="button"
          className="primary"
          disabled={!saveable || saving}
          onClick={async () => {
            setSaving(true);
            const saved = await save().finally(() => setSaving(false));
            setConfirming(null);
            if (saved) confirming();
          }}
        >
          Save changes
        </button>
      </div>
    </Modal>
  );
  return { leave, asking };
}
