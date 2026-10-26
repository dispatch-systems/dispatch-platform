import { useRef, useState } from 'react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { useUnsavedEdits } from '../../../../core/shell/frontend/runtime/useUnsavedEdits.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { renamePhone, type PhoneRow } from '../../api/client.js';

// Renaming one of the DSP's phones, as Dispatch or Station 2. A name changed and not saved is
// never just closed: Escape, the backdrop, the close button and Cancel ask to save it first.

export function RenameDialog({
  phone,
  onClose,
}: {
  phone: PhoneRow;
  /** Closes it, told whether the phone was renamed. */
  onClose: (renamed: boolean) => void;
}) {
  const [name, setName] = useState(phone.name);
  const changed = name.trim() !== phone.name;
  const rename = useAction(() => renamePhone(phone.id, name.trim()), { success: 'Phone renamed' });
  // Whether a save went through, so closing after it has the card read the phones again.
  const renamed = useRef(false);
  const save = async () => {
    const saved = await rename.run();
    if (saved) renamed.current = true;
    return saved;
  };
  const unsaved = useUnsavedEdits({
    what: 'This phone',
    dirty: changed,
    saveable: name.trim() !== '',
    save,
  });
  const close = () => unsaved.leave(() => onClose(renamed.current));
  return (
    <Modal title="Rename phone" onClose={close}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save().then((saved) => saved && onClose(true));
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
      {unsaved.asking}
    </Modal>
  );
}
