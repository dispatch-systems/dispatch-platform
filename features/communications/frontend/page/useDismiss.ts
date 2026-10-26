import { useEffect, useRef } from 'react';

/**
 * While `active`, leaves what a box holds under its button, as a form, on a press anywhere outside
 * the box, or on Escape, putting focus back on the button, as the shared Popover does its menu's:
 * the box's ref, and its button's.
 */
export function useDismiss(active: boolean, leave: () => void) {
  const box = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!active) return;
    const outside = (event: PointerEvent) => {
      if (!box.current?.contains(event.target as Node)) leave();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      leave();
      button.current?.focus();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
    };
  }, [active, leave]);
  return { box, button };
}
