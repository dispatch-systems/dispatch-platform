import { useRef, useState } from 'react';
import { useAction } from '../../../../../core/shell/frontend/runtime/useAction.js';
import { Modal } from '../../../../../core/shell/frontend/ui/index.js';
import { renamePhone, type PhoneRow } from '../../../api/client.js';

// Renaming one of the DSP's phones, as Dispatch or Station 2. A name changed and not saved is
// never just closed: Escape, the backdrop, the close button and Cancel ask "Save changes?" with
// only Save and Discard, as the role sheet does. Closing the browser tab drops it silently.

export function RenameDialog({
  phone,
  onClose,
}: {
  phone: PhoneRow;
  /** Closes it, told whether the phone was renamed. */
  onClose: (renamed: boolean) => void;
}) {
  const [name, setName] = useState(phone.name);
  const [confirming, setConfirming] = useState(false);
  const changed = name.trim() !== phone.name;
  const rename = useAction(() => renamePhone(phone.id, name.trim()), { success: 'Phone renamed' });
  // Whether a save went through, so closing after it has the card read the phones again.
  const renamed = useRef(false);
  const save = async () => {
    const saved = await rename.run();
    if (saved) {
      renamed.current = true;
      onClose(true);
    }
    return saved;
  };
  const close = () => (changed ? setConfirming(true) : onClose(renamed.current));
  return (
    <Modal title="Rename phone" onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label className="field">
          <span>Name</span>
          <input
            value={name}
            maxLength={60}
            required
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <div className="form-actions">
          <button type="button" onClick={close}>
            Cancel
          </button>
          <button className="primary" disabled={rename.busy || !name.trim()}>
            Save
          </button>
        </div>
      </form>
      {confirming && (
        <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
          <div className="form-actions">
            <button type="button" onClick={() => onClose(renamed.current)}>
              Discard changes
            </button>
            <button
              type="button"
              className="primary"
              disabled={rename.busy || !name.trim()}
              onClick={async () => {
                if (!(await save())) setConfirming(false);
              }}
            >
              Save changes
            </button>
          </div>
        </Modal>
      )}
    </Modal>
  );
}
