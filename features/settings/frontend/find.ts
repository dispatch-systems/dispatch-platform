import { useCallback, useEffect, useState, type RefCallback } from 'react';
import { addressedSection, followAddress, forgetSection } from './address.js';

// A section the address asks to point out, as a search result opening it does: once drawn, it
// is scrolled into view and outlined for a few seconds, or until the member does something
// first. The address forgets it at once, so opening the page again doesn't point it out.

/** How long a section stays pointed out, in milliseconds. */
const shown = 4000;

/** The section pointed out, and the ref its element takes, which scrolls to it once drawn. */
export type Found = { section: string; reveal: RefCallback<HTMLElement> };

/** The section the address asks to point out, while it is pointed out. */
export function useFound(): Found | undefined {
  const [asked, setAsked] = useState(() => {
    const section = addressedSection();
    return section ? { section } : undefined;
  });
  useEffect(() => {
    forgetSection();
    return followAddress(() => {
      const section = addressedSection();
      if (!section) return;
      forgetSection();
      setAsked({ section });
    });
  }, []);
  const reveal = useCallback(
    (element: HTMLElement | null) => {
      if (!element || !asked) return;
      const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
      element.scrollIntoView({ block: 'start', behavior: still ? 'auto' : 'smooth' });
      // What the member does from now on, not the key or click that opened it, ends it.
      const since = performance.now();
      const done = (event?: Event) => {
        if (!event || event.timeStamp > since) setAsked(undefined);
      };
      const timer = setTimeout(done, shown);
      window.addEventListener('pointerdown', done);
      window.addEventListener('keydown', done);
      return () => {
        clearTimeout(timer);
        window.removeEventListener('pointerdown', done);
        window.removeEventListener('keydown', done);
      };
    },
    [asked],
  );
  return asked && { section: asked.section, reveal };
}
