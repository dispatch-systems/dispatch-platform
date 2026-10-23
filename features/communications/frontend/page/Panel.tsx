import { useEffect, useRef, type ReactNode } from 'react';
import { X } from 'lucide-react';

// A panel beside what's open, a conversation's profile or a group's people: over it until its X,
// Escape, or a press anywhere else closes it; or, on a wide page (`docked`), a column of its own
// beside it, covering nothing, until its X or Escape does. Focus moves to its X as it opens. It
// slides in, and closed (`leaving`), slides back out, out of reach, before it's `gone`.

export function Panel({
  id,
  labelledBy,
  className,
  docked,
  leaving,
  close,
  gone,
  children,
}: {
  /** Its own ID, which its openers control. */
  id: string;
  /** The ID of what names it. */
  labelledBy: string;
  className: string;
  docked: boolean;
  /** Whether it's closed, sliding away. */
  leaving: boolean;
  /** Closes it: `refocus`, focus goes back to what opened it. */
  close: (refocus: boolean) => void;
  /** Says it has slid away. */
  gone: () => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLElement>(null);
  const closer = useRef<HTMLButtonElement>(null);
  useEffect(() => closer.current?.focus(), []);
  useEffect(() => {
    if (docked || leaving) return;
    // A press on what opens it is that control's own: it closes it.
    const pressed = ({ target }: PointerEvent) => {
      const opener = target instanceof Element && target.closest('[data-opens-profile]');
      if (!panel.current?.contains(target as Node) && !opener) close(false);
    };
    document.addEventListener('pointerdown', pressed);
    return () => document.removeEventListener('pointerdown', pressed);
  }, [docked, leaving, close]);
  return (
    <aside
      ref={panel}
      id={id}
      className={leaving ? `${className} leaving` : className}
      role="dialog"
      aria-labelledby={labelledBy}
      inert={leaving}
      onKeyDown={(event) => {
        if (event.key === 'Escape') close(true);
      }}
      onAnimationEnd={(event) => {
        if (leaving && event.target === event.currentTarget) gone();
      }}
    >
      <button
        ref={closer}
        className="icon-button close"
        aria-label="Close"
        onClick={() => close(true)}
      >
        <X size={16} />
      </button>
      {children}
    </aside>
  );
}
