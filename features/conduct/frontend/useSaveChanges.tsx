import { useEffect, useRef, useState } from 'react';
import { Modal } from '../../../core/shell/frontend/ui/index.js';
import { useLeaveGuard, type Leave } from '../../../core/shell/frontend/runtime/leave.js';

/**
 * An editor's unsaved changes, kept until someone saves or discards them, as the editor is left
 * through `leave` or the page it's on moves on (Settings asks its sections' editors through
 * `useLeaveGuard`). With none (`dirty` false), either goes on at once; with some, `asking`,
 * drawn while it asks, offers Save or Discard and nothing else, as
 * `features/team/frontend/RoleSheet.tsx` does, and Save goes on once `save` has saved. Closing
 * the browser tab drops them silently.
 */
export function useSaveChanges({
  what,
  dirty,
  saveable,
  save,
}: {
  /** What has the changes, as the question names it: "This category". */
  what: string;
  dirty: boolean;
  saveable: boolean;
  /** Saves the changes; whether leaving should go on. */
  save: () => Promise<boolean>;
}) {
  const [then, setThen] = useState<(() => void) | null>(null);
  const [saving, setSaving] = useState(false);
  const leave: Leave = (next) => (dirty ? setThen(() => next) : next());
  const guard = useRef<Leave | null>(null);
  useEffect(() => {
    guard.current = leave;
  });
  useLeaveGuard(guard);
  const asking = then && (
    <Modal title="Save changes?" dismissible={false} onClose={() => {}} above>
      <p>{what} has unsaved changes.</p>
      <div className="form-actions">
        <button
          type="button"
          disabled={saving}
          onClick={() => {
            setThen(null);
            then();
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
            const goOn = await save().finally(() => setSaving(false));
            setThen(null);
            if (goOn) then();
          }}
        >
          Save changes
        </button>
      </div>
    </Modal>
  );
  return { leave, asking };
}
