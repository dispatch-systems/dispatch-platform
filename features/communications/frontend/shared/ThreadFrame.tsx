import { useCallback, useLayoutEffect, useRef, type ReactNode } from 'react';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import type { ConversationViewport } from '../conversations/reads.js';
import { More, TryAgain } from './More.js';
import { PhotoViewing } from './PhotoViewer.js';
import './thread-frame.css';

const LATEST_PX = 80;
const EDGE_PX = 240;
/** Bounded conversation paging. Groups without this keep their existing append behavior. */
export type WindowThread = {
  hasAfter: boolean;
  busy: boolean;
  readNewer: () => void;
  readLatest: () => void;
  viewport: (read: () => ConversationViewport) => () => void;
  resume: () => void;
};
type Part = { id: string; part: string; top: number; place?: string };

export function ThreadFrame({
  items,
  latest,
  more,
  readOlder,
  olderFailed,
  window,
  viewport,
  resume,
  children,
}: {
  items: readonly { id: string; ts?: string }[];
  latest?: number;
  more: boolean;
  readOlder: () => void;
  olderFailed: boolean;
  window?: WindowThread;
  viewport?: WindowThread['viewport'];
  resume?: () => void;
  children: ReactNode;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const current = {
    items,
    more,
    readOlder,
    olderFailed,
    window,
    resume: resume ?? window?.resume,
    managed: Boolean(viewport || window),
  };
  const state = useRef(current);
  state.current = current;
  const mounted = useRef(false);
  const before = useRef({ first: '', height: 0, top: 0, latest: true, parts: [] as Part[] });
  const activity = useRef(0);
  const explicitLatest = useRef(false);
  const direction = useRef(-1);
  const serial = useRef(0);
  const attempted = useRef('');
  const scheduled = useRef<number | undefined>(undefined);
  const parts = useCallback(() => {
    const element = scroller.current;
    if (!element || !state.current.managed) return [];
    const places = new Map(state.current.items.map((item) => [item.id, item.ts]));
    const top = element.getBoundingClientRect().top;
    const bottom = top + element.clientHeight;
    return [...element.querySelectorAll<HTMLElement>('[data-message-id][data-message-part]')]
      .filter((part) => {
        const box = part.getBoundingClientRect();
        return box.bottom > top && box.top < bottom;
      })
      .map((part) => ({
        id: part.dataset.messageId!,
        part: part.dataset.messagePart!,
        top: part.getBoundingClientRect().top - top,
        place: places.get(part.dataset.messageId!),
      }));
  }, []);
  const blocked = useCallback(() => {
    const element = scroller.current;
    const selection = globalThis.getSelection();
    const focused = document.activeElement;
    let selecting = false;
    if (element && selection && !selection.isCollapsed) {
      for (let index = 0; index < selection.rangeCount; index++) {
        if (selection.getRangeAt(index).intersectsNode(element)) {
          selecting = true;
          break;
        }
      }
    }
    return (
      activity.current > 0 ||
      Boolean(
        focused instanceof Element && element?.contains(focused) && focused.closest('.msg-group'),
      ) ||
      selecting
    );
  }, []);
  const capture = useCallback(() => {
    const element = scroller.current;
    if (!element) return;
    before.current = {
      ...before.current,
      height: element.scrollHeight,
      top: element.scrollTop,
      parts: parts(),
      latest:
        !state.current.window?.hasAfter &&
        element.scrollHeight - element.scrollTop - element.clientHeight < LATEST_PX,
    };
  }, [parts]);
  const edges = useCallback(() => {
    const element = scroller.current;
    const current = state.current;
    if (!element || !current.window || current.window.busy || current.olderFailed || blocked())
      return;
    const older = direction.current < 0 && current.more && element.scrollTop < EDGE_PX;
    const newer =
      direction.current > 0 &&
      current.window.hasAfter &&
      element.scrollHeight - element.scrollTop - element.clientHeight < EDGE_PX;
    if (!older && !newer) return;
    // Semantic endpoints, never randomized continuation strings. A stalled range cannot spin.
    const key = `${current.items[0]?.id}:${current.items.at(-1)?.id}:${older ? 'older' : 'newer'}`;
    if (attempted.current === key) return;
    attempted.current = key;
    if (older) current.readOlder();
    else current.window.readNewer();
  }, [blocked]);
  const wake = useCallback(() => {
    if (!mounted.current) return;
    if (scheduled.current !== undefined) cancelAnimationFrame(scheduled.current);
    scheduled.current = requestAnimationFrame(() => {
      scheduled.current = undefined;
      capture();
      if (!blocked()) {
        state.current.resume?.();
        edges();
      }
    });
  }, [capture, blocked, edges]);
  const active = useCallback(
    (on: boolean) => {
      activity.current += on ? 1 : -1;
      if (!on) wake();
    },
    [wake],
  );
  const bind = viewport ?? window?.viewport;
  useLayoutEffect(
    () =>
      bind?.(() => {
        const visible = parts();
        const element = scroller.current;
        const latest = Boolean(
          element &&
          !state.current.window?.hasAfter &&
          element.scrollHeight - element.scrollTop - element.clientHeight < LATEST_PX,
        );
        return {
          anchor: visible[0]?.id,
          visible: visible.map((part) => part.id),
          latest,
          blocked: blocked(),
          serial: serial.current,
        };
      }),
    [bind, parts, blocked],
  );

  const restore = useCallback(() => {
    const element = scroller.current;
    if (!element) return;
    const was = before.current;
    const current = state.current;
    if ((was.latest || explicitLatest.current) && !current.window?.hasAfter) {
      element.scrollTop = element.scrollHeight;
      explicitLatest.current = false;
    } else if (bind) {
      const candidates = [
        ...element.querySelectorAll<HTMLElement>('[data-message-id][data-message-part]'),
      ];
      const anchor = was.parts
        .map((old) => ({
          old,
          element: candidates.find(
            (part) => part.dataset.messageId === old.id && part.dataset.messagePart === old.part,
          ),
        }))
        .find((part) => part.element);
      if (anchor?.element)
        element.scrollTop +=
          anchor.element.getBoundingClientRect().top -
          element.getBoundingClientRect().top -
          anchor.old.top;
      else {
        // If every visible part was removed, use the nearest surviving transport position.
        const old = was.parts[0];
        const place = old?.place?.padStart(24, '0');
        const nearest = place
          ? (current.items.find((item) => item.ts && item.ts.padStart(24, '0') >= place) ??
            current.items.at(-1))
          : undefined;
        const replacement =
          nearest && candidates.find((part) => part.dataset.messageId === nearest.id);
        if (replacement && old)
          element.scrollTop +=
            replacement.getBoundingClientRect().top - element.getBoundingClientRect().top - old.top;
      }
    } else if (was.first && current.items[0]?.id !== was.first && !was.latest) {
      element.scrollTop = was.top + element.scrollHeight - was.height;
    }
    before.current.first = current.items[0]?.id ?? '';
    capture();
  }, [capture, bind]);
  useLayoutEffect(() => {
    restore();
    // A receipt can remove its focused Retry control without a browser blur event.
    wake();
    const element = scroller.current;
    // Continue only to fill a short viewport; ordinary shifts await the user's next scroll.
    if (window && element && element.scrollHeight <= element.clientHeight + 1) edges();
  }, [items, window, restore, edges, wake]);
  useLayoutEffect(() => {
    if (!latest) return;
    if (state.current.window) {
      explicitLatest.current = true;
      attempted.current = '';
      state.current.window.readLatest();
    } else {
      const element = scroller.current;
      if (element) {
        element.scrollTop = element.scrollHeight;
        capture();
      }
    }
  }, [latest, capture]);
  useLayoutEffect(() => {
    if (!state.current.window) return;
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => restore());
    observer.observe(element);
    for (const part of element.querySelectorAll('[data-message-id][data-message-part]'))
      observer.observe(part);
    return () => observer.disconnect();
  }, [bind, items, restore]);
  useLayoutEffect(() => {
    mounted.current = true;
    document.addEventListener('selectionchange', wake);
    return () => {
      mounted.current = false;
      document.removeEventListener('selectionchange', wake);
      if (scheduled.current !== undefined) cancelAnimationFrame(scheduled.current);
    };
  }, [wake]);
  const scrolled = () => {
    const element = scroller.current;
    if (!element) return;
    const delta = element.scrollTop - before.current.top;
    if (delta) {
      serial.current++;
      direction.current = delta < 0 ? -1 : 1;
      // Explicit latest must not override a subsequent user navigation.
      explicitLatest.current = false;
    }
    capture();
    if (delta) {
      edges();
      wake();
    }
  };
  return (
    <div
      className={window ? 'thread windowed' : 'thread'}
      ref={scroller}
      onScroll={scrolled}
      onFocusCapture={wake}
      onBlurCapture={wake}
    >
      {more && !window && <More read={readOlder} failed={olderFailed} />}
      {window && olderFailed && (
        <TryAgain
          retry={() => {
            attempted.current = '';
            readOlder();
          }}
        />
      )}
      <div className="thread-messages">
        <PhotoViewing activity={active}>{children}</PhotoViewing>
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
