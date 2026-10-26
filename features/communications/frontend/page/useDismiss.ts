import { useEffect, type RefObject } from 'react';

/** While `active`, leaves what `box` holds, as a form under a button, on Escape or a press anywhere
 * outside it. */
export function useDismiss(box: RefObject<HTMLElement | null>, active: boolean, leave: () => void) {
  useEffect(() => {
    if (!active) return;
    const outside = (event: PointerEvent) => {
      if (!box.current?.contains(event.target as Node)) leave();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') leave();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
    };
  }, [box, active, leave]);
}
