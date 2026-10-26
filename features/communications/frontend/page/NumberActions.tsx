import { useCallback, useEffect, useRef, useState } from 'react';
import { Copy, Ellipsis, Phone, PhoneOff, UserPlus } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
import { blockNumber, saveContact } from '../../api/client.js';
import { toClipboard } from '../../../../core/shell/frontend/runtime/clipboard.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';

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
  const [naming, setNaming] = useState(false);
  const named = useCallback(() => setNaming(false), []);
  // Its words, which on a phone only its mark shows.
  const saving = conversation.contact ? 'Edit contact' : 'Save contact';
  const copy = useAction(() => toClipboard(conversation.number ?? ''), {
    success: 'Number copied',
  });
  const block = useAction(async (blocked: boolean) =>
    show(await blockNumber(conversation.id, blocked)),
  );
  return (
    <>
      {!conversation.with.code && (
        <div className="save-contact">
          <button
            aria-label={saving}
            aria-expanded={naming}
            onClick={() => setNaming((open) => !open)}
          >
            <UserPlus size={15} />
            <span>{saving}</span>
          </button>
          {naming && <ContactForm conversation={conversation} show={show} close={named} />}
        </div>
      )}
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

/** The name to save for a conversation's number, under Save contact; it closes on Cancel, Escape
 * or a press anywhere else. */
function ContactForm({
  conversation,
  show,
  close,
}: {
  conversation: Conversation;
  show: (conversation: Conversation) => void;
  close: () => void;
}) {
  const [name, setName] = useState(conversation.contact ? conversation.with.name : '');
  const form = useRef<HTMLFormElement>(null);
  const save = useAction(async () => {
    show(await saveContact(conversation.id, name.trim()));
    close();
  });
  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (!form.current?.parentElement?.contains(event.target as Node)) close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
    };
  }, [close]);
  return (
    <form
      ref={form}
      className="contact-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (name.trim() && !save.busy) void save.run();
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
        <button type="button" onClick={close}>
          Cancel
        </button>
        <button type="submit" className="primary" disabled={!name.trim() || save.busy}>
          Save
        </button>
      </div>
    </form>
  );
}
