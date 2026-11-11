import type { ReactNode } from 'react';
import { Modal } from './Modal.js';

/**
 * Asks before an editor with unsaved changes closes, offering Discard or Save and nothing else:
 * Escape and a press outside it do nothing. A save that fails closes it alone (`stay`), back to
 * the edits.
 */
export function SaveChangesDialog({
  children,
  saveable,
  discard,
  save,
  stay,
}: {
  /** What has the changes, where it says. */
  children?: ReactNode;
  /** Whether Save may be chosen now. */
  saveable: boolean;
  discard: () => void;
  /** Saves the changes; whether they were. */
  save: () => Promise<boolean>;
  stay: () => void;
}) {
  return (
    <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
      {children && <p>{children}</p>}
      <div className="form-actions">
        <button type="button" onClick={discard}>
          Discard changes
        </button>
        <button
          type="button"
          className="primary"
          disabled={!saveable}
          onClick={async () => {
            if (!(await save())) stay();
          }}
        >
          Save changes
        </button>
      </div>
    </Modal>
  );
}
