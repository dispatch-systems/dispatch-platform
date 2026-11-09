import { Modal } from '../../../core/shell/frontend/ui/index.js';

/**
 * Asks, as an editor with unsaved changes is left, whether to save them or discard them, and
 * nothing else, as `features/team/frontend/RoleSheet.tsx` asks. Closing the browser tab drops
 * them silently.
 */
export function SaveChanges({
  what,
  saveable,
  busy,
  save,
  discard,
}: {
  /** What has the changes, as the question names it: "This category". */
  what: string;
  saveable: boolean;
  busy: boolean;
  save: () => void;
  discard: () => void;
}) {
  return (
    <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
      <p>{what} has unsaved changes.</p>
      <div className="form-actions">
        <button type="button" disabled={busy} onClick={discard}>
          Discard changes
        </button>
        <button type="button" className="primary" disabled={!saveable || busy} onClick={save}>
          Save changes
        </button>
      </div>
    </Modal>
  );
}
