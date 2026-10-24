import { useLayoutEffect, useState, type RefObject } from 'react';

// How the page is laid out at its own width, the same widths communications.css's container
// queries go by: one pane at a time on a phone or a narrow window, the list beside the
// conversation, and on a page wide enough for the list, the profile and the conversation
// between them (`dockAt`, display.ts), the profile, open, beside the conversation too.

/** Narrower than this, one pane at a time: a 1024px window, with the app's sidebar, has two. */
export const BESIDE = 680;

export type Layout = 'phone' | 'beside' | 'docked';

/** The layout at `page`'s width, as it changes, docking from `dockAt`; none until it's measured. */
export function useLayout(page: RefObject<HTMLElement | null>, dockAt: number) {
  const [width, setWidth] = useState<number>();
  useLayoutEffect(() => {
    const element = page.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry!.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, [page]);
  if (width === undefined) return undefined;
  return width < BESIDE ? 'phone' : width < dockAt ? 'beside' : 'docked';
}
