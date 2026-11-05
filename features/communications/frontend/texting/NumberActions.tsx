import { Copy, Ellipsis, Phone, PhoneOff, UserMinus } from 'lucide-react';
import type { Conversation, RouteDriver } from '../../api/index.js';
import { blockNumber, removeFromPerson } from '../../api/client.js';
import { toClipboard } from '../../../../core/shell/frontend/runtime/clipboard.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import { SaveContact } from './SaveContact.js';
import './number-actions.css';

// What someone at the DSP does with the number a conversation texts: add it to a driver or save a
// name for it, shared with everyone at the DSP, while Driver Match knows no one by it; take it off
// the driver it was added to; copy it; block or unblock it.

export function NumberActions({
  conversation,
  drivers,
  show,
}: {
  conversation: Conversation;
  /** Today's drivers, listed first to add the number to. */
  drivers: readonly RouteDriver[];
  /** Shows the conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
}) {
  const copy = useAction(() => toClipboard(conversation.number ?? ''), {
    success: 'Number copied',
  });
  const block = useAction(async (blocked: boolean) =>
    show(await blockNumber(conversation.id, blocked)),
  );
  const remove = useAction(async (code: string) =>
    show(await removeFromPerson(conversation.id, code)),
  );
  return (
    <>
      {!conversation.with.code && (
        <SaveContact conversation={conversation} drivers={drivers} show={show} />
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
        {conversation.added && conversation.with.code && (
          <button onClick={() => void remove.run(conversation.with.code!)}>
            <UserMinus size={15} />
            Remove from {conversation.with.name}
          </button>
        )}
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
