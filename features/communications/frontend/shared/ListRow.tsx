import type { ReactNode } from 'react';
import { Check } from 'lucide-react';
import './list-row.css';

// A row of the list, a conversation's, a group's or a driver's on Today's Routes: whom it's with,
// when it was last written in, what was written last, and what else it shows below that; while it
// waits, how long, and a check that marks it done without a reply, shown as it's pointed at or
// reached by the keyboard beside the conversation.

export function ListRow({
  className,
  selected,
  waits,
  face,
  name,
  time,
  wait,
  line,
  below,
  open,
  done,
}: {
  /** What kind of row it is, beside `convo`. */
  className?: string;
  selected: boolean;
  waits: boolean;
  face: ReactNode;
  name: ReactNode;
  /** When it was last written in. */
  time?: ReactNode;
  /** How long it has waited, while it waits. */
  wait?: ReactNode;
  /** What was written last, or who is writing. */
  line?: ReactNode;
  below?: ReactNode;
  open: () => void;
  /** Marks it done, while it waits. */
  done?: () => void;
}) {
  const classes = ['convo', className, selected && 'selected', waits && 'waits'];
  return (
    <li className={classes.filter(Boolean).join(' ')}>
      <button className="convo-open" aria-current={selected || undefined} onClick={open}>
        {face}
        <span className="convo-body">
          <span className="line1">
            <strong>{name}</strong>
            {time && <time>{time}</time>}
          </span>
          {(line || wait) && (
            <span className="line2">
              {line}
              {wait}
            </span>
          )}
          {below && <span className="line3">{below}</span>}
        </span>
      </button>
      {waits && done && (
        <button className="convo-done" aria-label="Done" onClick={done}>
          <Check size={16} />
        </button>
      )}
    </li>
  );
}
