import type { ButtonHTMLAttributes, ReactNode } from 'react';
import './message-run.css';

// A run of messages one after another from the same writer, as a conversation and a group show
// them: the writer's face beside the messages under their name, the other party's on the left
// (`in`) and the DSP's on the right (`out`), as the member chose in Message display; and under a
// message, when it was written and how it went.

export function MessageRun({
  direction,
  face,
  sender,
  after,
  className,
  children,
}: {
  direction: 'in' | 'out';
  face: ReactNode;
  /** Whom it's by, above its messages, when that shows. */
  sender?: ReactNode;
  /** What shows beside the run, past its messages. */
  after?: ReactNode;
  /** What kind of run it is, beside its direction. */
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={['msg-group', direction, className].filter(Boolean).join(' ')}>
      {face}
      <div className="stack">
        {sender}
        {children}
      </div>
      {after}
    </div>
  );
}

/**
 * Whom a run is by: their name, when it began while the messages are plain (`at`), and what else
 * goes with it; a button doing `onClick`, when given.
 */
export function SenderLine({
  name,
  at,
  onClick,
  children,
}: {
  name: ReactNode;
  at?: ReactNode;
  onClick?: () => void;
  children?: ReactNode;
}) {
  const line = (
    <>
      <span className="sender-name">{name}</span>
      {at && <span className="sent-at">{at}</span>}
      {children}
    </>
  );
  return onClick ? (
    <button className="sender" onClick={onClick}>
      {line}
    </button>
  ) : (
    <div className="sender">{line}</div>
  );
}

/** A run's face as a button, left out of Tab's order: the name it's beside reaches it. */
export function FaceButton({
  label,
  children,
  ...button
}: { label: string; children: ReactNode } & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button className="face-button" tabIndex={-1} aria-label={label} {...button}>
      {children}
    </button>
  );
}

/** Under a message: when it was written, while `time` shows, and how it went (`children`). */
export function MessageMeta({ time, children }: { time?: ReactNode; children?: ReactNode }) {
  if (!time && !children) return null;
  return (
    <div className="meta">
      {time && (
        <span className="at">
          {time}
          {children && ' · '}
        </span>
      )}
      {children}
    </div>
  );
}
