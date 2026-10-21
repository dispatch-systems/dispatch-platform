import { useLayoutEffect, useState, type RefObject } from 'react';

// How the page is laid out at its own width, the same widths communications.css's container
// queries go by: one pane at a time on a phone, the list beside the conversation, and on a wide
// page the profile, open, beside the conversation too. A hundredth of that width (`--cq`) sizes
// what lies outside the page's container, as communications.css says.

/** Narrower than this, one pane at a time. */
const BESIDE = 760;
/** From this wide, the profile, open, is a column of its own. */
const DOCKED = 1500;

export type Layout = 'phone' | 'beside' | 'docked';

/** The layout at `page`'s width, as it changes; none until it's measured. */
export function useLayout(page: RefObject<HTMLElement | null>) {
  const [layout, setLayout] = useState<Layout>();
  useLayoutEffect(() => {
    const element = page.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const width = entry!.contentRect.width;
      element.style.setProperty('--cq', `${width / 100}px`);
      setLayout(width < BESIDE ? 'phone' : width < DOCKED ? 'beside' : 'docked');
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [page]);
  return layout;
}
