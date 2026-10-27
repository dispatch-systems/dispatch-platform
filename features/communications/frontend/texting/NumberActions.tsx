import { Copy, Ellipsis, Phone, PhoneOff } from 'lucide-react';
import type { Conversation } from '../../api/index.js';
import { blockNumber } from '../../api/client.js';
import { toClipboard } from '../../../../core/shell/frontend/runtime/clipboard.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import { SaveContact } from './SaveContact.js';

// What someone at the DSP does with the number a conversation texts: save a name for it, shared
// with everyone at the DSP, while Driver Match knows no one by it; copy it; block or unblock it.

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
