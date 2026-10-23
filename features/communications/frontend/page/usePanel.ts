import { useCallback, useId, useRef, useState } from 'react';
import { opensProfile } from './parts.js';

/**
 * A panel beside what's open, and what opens it: whether it's open, from the first when `kept`
 * open, as the member may have the profile; whether it's drawn, which it stays as it slides away
 * once closed, until `gone`; whether it's `still`, kept open as it was first drawn, there without
 * sliding in or taking focus until it's next opened or closed; its ID, what makes a button open
 * and close it, and closing it, which puts focus back on what opened it, or on `who`, the
 * header's opener, when that's gone.
 */
export function usePanel(kept = false) {
  const [open, setOpen] = useState(kept);
  const [drawn, setDrawn] = useState(kept);
  const [still, setStill] = useState(kept);
  const id = useId();
  const opener = useRef<HTMLElement | null>(null);
  const who = useRef<HTMLButtonElement>(null);
  const toggle = useCallback((from: HTMLElement) => {
    opener.current = from;
    setOpen((open) => !open);
    setDrawn(true);
    setStill(false);
  }, []);
  const close = useCallback((refocus: boolean) => {
    setOpen(false);
    setStill(false);
    if (refocus) (opener.current?.isConnected ? opener.current : who.current)?.focus();
  }, []);
  const gone = useCallback(() => setDrawn(false), []);
  return { open, drawn, still, gone, id, who, close, opens: opensProfile(id, open, toggle) };
}
