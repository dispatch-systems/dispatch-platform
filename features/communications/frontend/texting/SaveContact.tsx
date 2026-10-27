import { useState } from 'react';
import { UserPlus } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
import { saveContact } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Modal, Popover } from '../../../../core/shell/frontend/ui/index.js';

/** The most a contact's name may be, as the server keeps it. */
const NAME_MOST = 80;

/**
 * Save contact, or Edit contact once a name is saved, and the name to save under it, which closes
 * on Cancel, Escape (focus going back to the button), a press anywhere else or the button again.
 * A name changed and not saved is never just closed: it asks "Save changes?" with only Save and
 * Discard first, as the role sheet does.
 */
export function SaveContact({
  conversation,
  show,
}: {
  conversation: Conversation;
  show: (conversation: Conversation) => void;
}) {
  const saved = conversation.contact ? conversation.with.name : '';
  // The name being written, while it is.
  const [name, setName] = useState<string>();
  const [confirming, setConfirming] = useState(false);
  const changed = name !== undefined && name.trim() !== saved;
  const closed = () => {
    setName(undefined);
    setConfirming(false);
  };
  const leave = () => (changed ? setConfirming(true) : closed());
  const save = useAction(async () => {
    show(await saveContact(conversation.id, name!.trim()));
    closed();
  });
  // Its words, which on a phone only its mark shows.
  const label = conversation.contact ? 'Edit contact' : 'Save contact';
  const saveable = Boolean(name?.trim()) && !save.busy;
  return (
    <>
      <Popover
        className="save-contact"
        panelClassName="contact-panel"
        label={label}
        trigger={
          <>
            <UserPlus size={15} />
            <span>{label}</span>
          </>
        }
        open={name !== undefined}
        onRequestOpen={() => setName(saved)}
        onRequestClose={leave}
      >
        <form
          className="contact-form"
          onSubmit={(event) => {
            event.preventDefault();
            if (saveable) void save.run();
          }}
        >
          <label>
            <span>Name</span>
            <input
              autoFocus
              value={name}
              maxLength={NAME_MOST}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <div className="contact-form-buttons">
            <button type="button" onClick={leave}>
              Cancel
            </button>
            <button type="submit" className="primary" disabled={!saveable}>
              Save
            </button>
          </div>
        </form>
      </Popover>
      {confirming && (
        <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
          <div className="form-actions">
            <button type="button" onClick={closed}>
              Discard changes
            </button>
            <button
              type="button"
              className="primary"
              disabled={!saveable}
              onClick={async () => {
                if (!(await save.run())) setConfirming(false);
              }}
            >
              Save changes
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
