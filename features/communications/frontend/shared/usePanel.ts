import { useCallback, useId, useRef, useState, type MouseEvent } from 'react';

/** What makes a button open the panel `id` beside what's open and close it again. */
function opensPanel(id: string, open: boolean, toggle: (from: HTMLElement) => void) {
  return {
    'data-opens-profile': '',
    'aria-haspopup': 'dialog',
    'aria-expanded': open,
    'aria-controls': open ? id : undefined,
    onClick: (event: MouseEvent<HTMLElement>) => toggle(event.currentTarget),
  } as const;
}
export type OpensProfile = ReturnType<typeof opensPanel>;

/**
 * A panel beside what's open, and what opens it: whether it's open, as `kept` open, as the member
 * may have the profile, until someone opens or closes it; whether it's drawn, which it stays as it
 * slides away once closed, until `gone`; whether it's `still`, open as kept, there without sliding
 * in or taking focus; its ID, what makes a button open and close it, and closing it, which puts
 * focus back on what opened it, or on `who`, the header's opener, when that's gone.
 */
export function usePanel(kept = false) {
  // Opened or closed by hand, once it has been; until then, as kept.
  const [chosen, setChosen] = useState<boolean>();
  const open = chosen ?? kept;
  const [sliding, setSliding] = useState(false);
  const id = useId();
  const opener = useRef<HTMLElement | null>(null);
  const who = useRef<HTMLButtonElement>(null);
  const toggle = useCallback(
    (from: HTMLElement) => {
      opener.current = from;
      setChosen(!open);
      setSliding(open);
    },
    [open],
  );
  const close = useCallback(
    (refocus: boolean) => {
      setChosen(false);
      setSliding(open);
      if (refocus) (opener.current?.isConnected ? opener.current : who.current)?.focus();
    },
    [open],
  );
  const gone = useCallback(() => setSliding(false), []);
  return {
    open,
    drawn: open || sliding,
    still: chosen === undefined && kept,
    gone,
    id,
    who,
    close,
    opens: opensPanel(id, open, toggle),
  };
}
