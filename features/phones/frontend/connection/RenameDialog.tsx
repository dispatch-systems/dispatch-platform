import { useState } from 'react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { renamePhone, type PhoneRow } from '../../api/client.js';

// Renaming one of the DSP's phones, as Dispatch or Station 2.

export function RenameDialog({
  phone,
  onClose,
}: {
  phone: PhoneRow;
  /** Closes it, told whether the phone was renamed. */
  onClose: (renamed: boolean) => void;
}) {
  const [name, setName] = useState(phone.name);
  const rename = useAction(
    async () => {
      await renamePhone(phone.id, name.trim());
      onClose(true);
    },
    { success: 'Phone renamed' },
  );
  return (
    <Modal title="Rename phone" onClose={() => onClose(false)}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void rename.run();
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
          <button type="button" onClick={() => onClose(false)}>
            Cancel
          </button>
          <button className="primary" disabled={rename.busy || !name.trim()}>
            Save
          </button>
        </div>
      </form>
    </Modal>
  );
}
