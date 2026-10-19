import { useEffect, useRef, useState } from 'react';
import { useLeaveGuard, type Leave } from '../../shell/frontend/runtime/leave.js';
import { Modal } from '../../shell/frontend/ui/index.js';

/**
 * A schedule's unsaved edits in its editor, kept until the member saves or discards them, as
 * the editor is left through `leave` or the page it's on moves on. With none (`dirty` false),
 * either goes on at once; with some, `asking`, drawn while it asks, has Save or Discard first,
 * and Save goes on once `save` has saved.
 */
export function useUnsavedSchedule({
  dirty,
  saveable,
  save,
}: {
  dirty: boolean;
  /** Whether Save may be chosen: the edits may be saved as they are. */
  saveable: boolean;
  /** Saves the edits; whether they were. */
  save: () => Promise<boolean>;
}) {
  const [confirming, setConfirming] = useState<(() => void) | null>(null);
  const leave: Leave = (then) => (dirty ? setConfirming(() => then) : then());
  const guard = useRef<Leave | null>(null);
  useEffect(() => {
    guard.current = leave;
  });
  useLeaveGuard(guard);
  const asking = confirming && (
    <Modal title="Save changes?" dismissible={false} onClose={() => {}} above>
      <p>This schedule has unsaved changes.</p>
      <div className="form-actions">
        <button
          type="button"
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
          disabled={!saveable}
          onClick={async () => {
            const saved = await save();
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
