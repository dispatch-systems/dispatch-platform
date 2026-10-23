import { useCallback, useId, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { opensProfile } from './parts.js';

/** Whether a panel is open, kept from one conversation to the next, and what changes it. */
export type Kept = { open: boolean; keep: Dispatch<SetStateAction<boolean>> };

/**
 * A panel beside what's open, and what opens it: whether it's open, its own or `kept`; whether
 * it's drawn, which it stays as it slides away once closed, until `gone`; whether it's `still`,
 * open as it was first drawn, kept so, there without sliding in or taking focus until it's next
 * opened or closed; its ID, what makes a button open and close it, and closing it, which puts
 * focus back on what opened it, or on `who`, the header's opener, when that's gone.
 */
export function usePanel(kept?: Kept) {
  const [own, setOwn] = useState(false);
  const open = kept ? kept.open : own;
  const setOpen = kept ? kept.keep : setOwn;
  const [drawn, setDrawn] = useState(open);
  const [still, setStill] = useState(open);
  const id = useId();
  const opener = useRef<HTMLElement | null>(null);
  const who = useRef<HTMLButtonElement>(null);
  const toggle = useCallback(
    (from: HTMLElement) => {
      opener.current = from;
      setOpen((open) => !open);
      setDrawn(true);
      setStill(false);
    },
    [setOpen],
  );
  const close = useCallback(
    (refocus: boolean) => {
      setOpen(false);
      setStill(false);
      if (refocus) (opener.current?.isConnected ? opener.current : who.current)?.focus();
    },
    [setOpen],
  );
  const gone = useCallback(() => setDrawn(false), []);
  return {
    open,
    drawn: drawn || open,
    still,
    gone,
    id,
    who,
    close,
    opens: opensProfile(id, open, toggle),
  };
}
