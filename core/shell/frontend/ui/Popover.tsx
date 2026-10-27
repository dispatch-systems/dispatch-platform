import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from 'react';

// Share dismissal listeners across row menus, including the gap before a native toggle reaches React.
// Each menu is kept with how to open it at a point, for a right-click on what it belongs to.
type OpenAt = (x?: number, y?: number) => void;
const popovers = new Map<HTMLDetailsElement, OpenAt>();
function dismiss(event: PointerEvent) {
  for (const element of popovers.keys())
    if (element.open && !element.contains(event.target as Node)) element.open = false;
}
function escape(event: globalThis.KeyboardEvent) {
  if (event.key !== 'Escape') return;
  for (const element of popovers.keys()) {
    if (!element.open) continue;
    element.open = false;
    element.querySelector('summary')!.focus();
  }
}
function register(element: HTMLDetailsElement, openAt: OpenAt) {
  if (!popovers.size) {
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('keydown', escape);
  }
  popovers.set(element, openAt);
  return () => {
    popovers.delete(element);
    if (!popovers.size) {
      document.removeEventListener('pointerdown', dismiss);
      document.removeEventListener('keydown', escape);
    }
  };
}

/**
 * Opens the menu inside the element right-clicked in place of the browser's own: an anchored
 * one at the pointer, or under its button when the keyboard asked.
 */
export function openContextMenu(event: ReactMouseEvent<HTMLElement>) {
  for (const [element, openAt] of popovers) {
    if (!event.currentTarget.contains(element)) continue;
    event.preventDefault();
    if (element.querySelector('.account-popover')!.contains(event.target as Node)) return;
    // Chromium marks the keyboard's with no button, the standard with no pointer type.
    const { button, pointerType } = event.nativeEvent as PointerEvent;
    if (button === -1 || pointerType === '') openAt();
    else openAt(event.clientX, event.clientY);
    return;
  }
}

/** What every popover has: a trigger, named `label`, and what its panel holds. */
type Opener = {
  label: string;
  trigger: ReactNode;
  children: ReactNode;
  className: string;
  triggerClassName?: string;
};
/** A menu, which opens and closes itself. */
type MenuProps = Opener & {
  /** Places the panel against the viewport so a scrolling or clipping ancestor cannot cut it off. */
  anchored?: boolean;
  /** Mount row actions only while their menu is open. The empty panel keeps dismissal safe. */
  lazy?: boolean;
};
/**
 * A panel holding a search field or a form, open while `open` is: its trigger asks to open it,
 * and its trigger, a press outside it or Escape ask to close it, so an editor can ask first
 * about what isn't saved.
 */
type PanelProps = Opener & {
  open: boolean;
  onRequestOpen: () => void;
  onRequestClose: () => void;
  /** The panel's class: it is a dialog, named as its trigger is. */
  panelClassName: string;
};

/**
 * What opens under its trigger and closes on Escape, focus going back to the trigger, or on a
 * press outside it: a menu, which closes too once an item is chosen; or, given `open` and asked
 * to open and close, a panel holding a search field or a form, which a press inside never closes.
 */
export function Popover(props: MenuProps | PanelProps) {
  return 'onRequestClose' in props ? <Panel {...props} /> : <Menu {...props} />;
}

/** A `<details>` menu that closes on Escape, on a press outside it, and once an item is chosen. */
function Menu({
  label,
  trigger,
  children,
  className,
  triggerClassName,
  anchored = false,
  lazy = false,
}: MenuProps) {
  const details = useRef<HTMLDetailsElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  // Where a right-click opened it, from its button, so the panel moves with the page.
  const [pointer, setPointer] = useState<{ x: number; y: number }>();
  useLayoutEffect(() => {
    if (!open) return;
    const element = details.current!;
    const summary = element.querySelector('summary')!;
    const menu = panel.current!;
    const place = () => {
      const button = summary.getBoundingClientRect();
      const anchor = pointer
        ? new DOMRect(button.left + pointer.x, button.top + pointer.y)
        : button;
      const gap = pointer ? 0 : 4;
      if (
        anchor.bottom < 0 ||
        anchor.top > window.innerHeight ||
        anchor.right < 0 ||
        anchor.left > window.innerWidth
      ) {
        element.open = false;
        return;
      }
      const width = menu.offsetWidth,
        height = menu.offsetHeight;
      const left = Math.max(
        8,
        Math.min(pointer ? anchor.left : anchor.right - width, window.innerWidth - width - 8),
      );
      const below = anchor.bottom + gap;
      const top =
        below + height <= window.innerHeight - 8 ? below : Math.max(8, anchor.top - height - gap);
      Object.assign(menu.style, { left: `${left}px`, top: `${top}px`, visibility: 'visible' });
    };
    if (anchored) {
      place();
      window.addEventListener('resize', place);
      window.addEventListener('scroll', place, { capture: true, passive: true });
    }
    return () => {
      if (anchored) {
        menu.style.visibility = 'hidden';
        window.removeEventListener('resize', place);
        window.removeEventListener('scroll', place, true);
      }
    };
  }, [open, anchored, pointer]);
  // The browser opens the menu before its toggle event reaches React, so these listen from
  // the start and read the element: a key pressed in that gap still closes it.
  useEffect(() => {
    const element = details.current!;
    return register(element, (x, y) => {
      const button = element.querySelector('summary')!.getBoundingClientRect();
      setPointer(x === undefined ? undefined : { x: x - button.left, y: y! - button.top });
      element.open = true;
    });
  }, []);
  return (
    <details
      ref={details}
      className={className}
      onToggle={(event) => {
        setOpen(event.currentTarget.open);
        if (!event.currentTarget.open) setPointer(undefined);
      }}
    >
      <summary className={triggerClassName} aria-label={label}>
        {trigger}
      </summary>
      <div
        ref={panel}
        className="account-popover"
        onClick={() => {
          details.current!.open = false;
        }}
      >
        {(!lazy || open) && children}
      </div>
    </details>
  );
}

/** A panel holding a search field or a form, open while its owner says. */
function Panel({
  label,
  trigger,
  children,
  className,
  triggerClassName,
  open,
  onRequestOpen,
  onRequestClose,
  panelClassName,
}: PanelProps) {
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const close = useRef(onRequestClose);
  useEffect(() => {
    close.current = onRequestClose;
  }, [onRequestClose]);
  // Whether Escape asked it to close: focus goes back to the trigger as it does.
  const escaped = useRef(false);
  useEffect(() => {
    if (!open) return;
    const [element, opener] = [root.current!, button.current!];
    const outside = (event: PointerEvent) => {
      if (!element.contains(event.target as Node)) close.current();
    };
    const escape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      escaped.current = true;
      close.current();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
      if (escaped.current) opener.focus();
      escaped.current = false;
    };
  }, [open]);
  return (
    <div ref={root} className={className}>
      <button
        ref={button}
        className={triggerClassName}
        aria-label={label}
        aria-expanded={open}
        onClick={() => (open ? onRequestClose() : onRequestOpen())}
      >
        {trigger}
      </button>
      {open && (
        <div className={panelClassName} role="dialog" aria-label={label}>
          {children}
        </div>
      )}
    </div>
  );
}
