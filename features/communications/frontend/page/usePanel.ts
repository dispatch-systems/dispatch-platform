import { useCallback, useId, useRef, useState } from 'react';
import { opensProfile } from './parts.js';

/**
 * A panel beside what's open, and what opens it: whether it's open, its ID, what makes a button
 * open and close it, and closing it, which puts focus back on what opened it, or on `who`, the
 * header's opener, when that's gone.
 */
export function usePanel() {
  const [open, setOpen] = useState(false);
  const id = useId();
  const opener = useRef<HTMLElement | null>(null);
  const who = useRef<HTMLButtonElement>(null);
  const toggle = useCallback((from: HTMLElement) => {
    opener.current = from;
    setOpen((open) => !open);
  }, []);
  const close = useCallback((refocus: boolean) => {
    setOpen(false);
    if (refocus) (opener.current?.isConnected ? opener.current : who.current)?.focus();
  }, []);
  return { open, id, who, close, opens: opensProfile(id, open, toggle) };
}
