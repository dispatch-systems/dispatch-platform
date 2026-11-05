import { useState } from 'react';
import { MessageSquareText } from 'lucide-react';
import { useQuickReplies } from '../../api/client.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import './quick-replies.css';

// The DSP's quick replies, in what's written: Quick replies, beside the smiley, opens them, and the
// one chosen goes in where the writer was, never sent by itself. Not there while the DSP has none.

export function QuickReplies({
  disabled,
  pick,
}: {
  disabled: boolean;
  /** Puts `reply` in what's written. */
  pick: (reply: string) => void;
}) {
  const replies = useQuickReplies().data?.replies ?? [];
  const [open, setOpen] = useState(false);
  if (!replies.length) return null;
  return (
    <Popover
      className="quick-replies"
      triggerClassName="quick-replies-button"
      panelClassName="quick-replies-panel"
      label="Quick replies"
      trigger={
        <>
          <MessageSquareText size={18} aria-hidden="true" />
          <span>Quick replies</span>
        </>
      }
      open={open}
      disabled={disabled}
      onRequestOpen={() => setOpen(true)}
      onRequestClose={() => setOpen(false)}
    >
      <ul>
        {replies.map((reply, n) => (
          <li key={n}>
            <button
              type="button"
              onClick={() => {
                setOpen(false);
                pick(reply);
              }}
            >
              {reply}
            </button>
          </li>
        ))}
      </ul>
    </Popover>
  );
}
