import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { More } from './More.js';
import './thread-frame.css';

// The messages of a conversation or a group, scrolling on their own: kept at the latest as more
// come, or where they were as older ones come above, which are read as its top nears (`More`).

/** How near its end it still counts as at the latest, in pixels. */
const LATEST_PX = 80;

export function ThreadFrame({
  items,
  after,
  more,
  readOlder,
  children,
}: {
  /** What it shows, in the order written: as they change, it keeps its place. */
  items: readonly { id: string }[];
  /** What shows below them, as who is writing, which keeps it at the latest too. */
  after?: readonly unknown[];
  more: boolean;
  readOlder: () => void;
  children: ReactNode;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  // Where it was before the latest change: at the latest message, or how far up, which older
  // messages coming above keep.
  const before = useRef({ first: '', height: 0, top: 0, latest: true });
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const first = items[0]?.id ?? '';
    const was = before.current;
    if (was.first && first !== was.first && !was.latest)
      element.scrollTop = was.top + element.scrollHeight - was.height;
    else if (was.latest) element.scrollTop = element.scrollHeight;
    before.current = { ...was, first, height: element.scrollHeight, top: element.scrollTop };
  }, [items, after]);
  const scrolled = () => {
    const element = scroller.current;
    if (!element) return;
    before.current = {
      ...before.current,
      height: element.scrollHeight,
      top: element.scrollTop,
      latest: element.scrollHeight - element.scrollTop - element.clientHeight < LATEST_PX,
    };
  };
  return (
    <div className="thread" ref={scroller} onScroll={scrolled}>
      {more && <More read={readOlder} />}
      <div className="thread-messages">{children}</div>
    </div>
  );
}

/** In a thread's place until it's read: loading, or nothing once it `failed`. */
export function ThreadPending({ failed }: { failed: boolean }) {
  return <div className="thread">{!failed && <Loading />}</div>;
}

/** The day the messages under it were written. */
export function DayLabel({ children }: { children: ReactNode }) {
  return <div className="day">{children}</div>;
}
