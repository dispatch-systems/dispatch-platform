import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { More, TryAgain } from './More.js';
import { PhotoViewing } from './PhotoViewer.js';
import './thread-frame.css';

// The messages of a conversation or a group, scrolling on their own: kept at the latest as more
// come, or where they were as older ones come above, which are read as its top nears (`More`);
// and what goes between them: the day they were written, and the time after a pause; and back at
// the latest as asked (`latest`). A read that failed is read again as Try again is pressed.

/** How near its end it still counts as at the latest, in pixels. */
const LATEST_PX = 80;

export function ThreadFrame({
  items,
  latest,
  more,
  readOlder,
  olderFailed,
  children,
}: {
  /** What it shows, in the order written: as they change, it keeps its place. */
  items: readonly { id: string }[];
  /** Goes to the latest as it changes, wherever it was. */
  latest?: number;
  more: boolean;
  readOlder: () => void;
  /** Whether the latest read of older ones failed. */
  olderFailed: boolean;
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
  }, [items]);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element || !latest) return;
    element.scrollTop = element.scrollHeight;
    before.current = { ...before.current, height: element.scrollHeight, top: element.scrollTop, latest: true };
  }, [latest]);
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
      {more && <More read={readOlder} failed={olderFailed} />}
      <div className="thread-messages">
        <PhotoViewing>{children}</PhotoViewing>
      </div>
    </div>
  );
}

/** In a thread's place until it's read: loading, or once it `failed`, Try again. */
export function ThreadPending({ failed, retry }: { failed: boolean; retry: () => void }) {
  return <div className="thread">{failed ? <TryAgain retry={retry} /> : <Loading />}</div>;
}

/** The day the messages under it were written. */
export function DayLabel({ children }: { children: ReactNode }) {
  return <div className="day">{children}</div>;
}

/** When the messages under it were written, after a pause (`paused`, messages.ts). */
export function PauseLabel({ children }: { children: ReactNode }) {
  return <div className="day pause">{children}</div>;
}
