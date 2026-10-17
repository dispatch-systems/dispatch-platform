import { useState } from 'react';
import { ConfirmDialog, Modal } from '../../../../core/shell/frontend/ui/index.js';
import type { DocumentsItem } from '../../api/client.js';

/**
/**
 * Asks for a name, to make something or rename it: a file's `extension` stays, unshown. It
 * closes as the name is given; the page shows the change while Google makes it.
 */
export function Naming({
  title,
  initial,
  extension = '',
  confirm,
  save,
  done,
}: {
  title: string;
  initial: string;
  extension?: string;
  confirm: string;
  save: (name: string) => void;
  done: () => void;
}) {
  const [name, setName] = useState(initial);
  return (
    <Modal title={title} onClose={done} initialFocus="input">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save(name.trim() + extension);
          done();
        }}
      >
        <label>
          Name
          <input
            value={name}
            maxLength={200 - extension.length}
            required
            onChange={(event) => setName(event.target.value)}
            onFocus={(event) => event.target.select()}
          />
        </label>
        <div className="form-actions">
          <button type="button" onClick={done}>
            Cancel
          </button>
          <button className="primary" disabled={!name.trim()}>
            {confirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function Trash({
  item,
  done,
  trash,
}: {
  item: DocumentsItem;
  done: () => void;
  trash: () => void;
}) {
  return (
    <ConfirmDialog
      title={`Move ${item.name} to the trash?`}
      confirm="Move to trash"
      tone="danger"
      onConfirm={() => {
        trash();
        done();
      }}
      onCancel={done}
    >
      {item.kind === 'folder'
        ? 'It and everything in it go to the trash of the Google account that holds Documents, for 30 days. Its owner can restore them from Google Drive.'
        : 'It goes to the trash of the Google account that holds Documents, for 30 days. Its owner can restore it from Google Drive.'}
    </ConfirmDialog>
  );
}
