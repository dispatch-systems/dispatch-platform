import { useCallback, useRef, useState } from 'react';
import { Copy, Ellipsis, Phone, PhoneOff, UserPlus } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
import { blockNumber, saveContact } from '../../api/client.js';
import { toClipboard } from '../../../../core/shell/frontend/runtime/clipboard.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Modal, Popover } from '../../../../core/shell/frontend/ui/index.js';
import { useDismiss } from './useDismiss.js';

// What someone at the DSP does with the number a conversation texts: save a name for it, shared
// with everyone at the DSP, while Driver Match knows no one by it; copy it; block or unblock it.

/** The most a contact's name may be, as the server keeps it. */
const NAME_MOST = 80;

export function NumberActions({
  conversation,
  show,
}: {
  conversation: Conversation;
  /** Shows the conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
}) {
  const copy = useAction(() => toClipboard(conversation.number ?? ''), {
    success: 'Number copied',
  });
  const block = useAction(async (blocked: boolean) =>
    show(await blockNumber(conversation.id, blocked)),
  );
  return (
    <>
      {!conversation.with.code && <SaveContact conversation={conversation} show={show} />}
      <Popover
        className="number-menu"
        triggerClassName="number-menu-button"
        label="More"
        trigger={<Ellipsis size={15} />}
      >
        <button onClick={() => void copy.run()}>
          <Copy size={15} />
          Copy number
        </button>
        {conversation.blocked ? (
          <button onClick={() => void block.run(false)}>
            <Phone size={15} />
            Unblock
          </button>
        ) : (
          <button className="danger" onClick={() => void block.run(true)}>
            <PhoneOff size={15} />
            Block
          </button>
        )}
      </Popover>
    </>
  );
}

/**
 * Save contact, or Edit contact once a name is saved, and the name to save under it, which closes
 * on Cancel, Escape, a press anywhere else or the button again. A name changed and not saved is
 * never just closed: it asks "Save changes?" with only Save and Discard first, as the role sheet
 * does.
 */
function SaveContact({
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
  const editor = useRef<HTMLDivElement>(null);
  const changed = name !== undefined && name.trim() !== saved;
  const closed = useCallback(() => {
    setName(undefined);
    setConfirming(false);
  }, []);
  const leave = useCallback(() => (changed ? setConfirming(true) : closed()), [changed, closed]);
  const save = useAction(async () => {
    show(await saveContact(conversation.id, name!.trim()));
    closed();
  });
  const naming = name !== undefined;
  useDismiss(editor, naming && !confirming, leave);
  // Its words, which on a phone only its mark shows.
  const label = conversation.contact ? 'Edit contact' : 'Save contact';
  const saveable = Boolean(name?.trim()) && !save.busy;
  return (
    <div ref={editor} className="save-contact">
      <button
        aria-label={label}
        aria-expanded={naming}
        onClick={() => (naming ? leave() : setName(saved))}
      >
        <UserPlus size={15} />
        <span>{label}</span>
      </button>
      {naming && (
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
      )}
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
    </div>
  );
}
