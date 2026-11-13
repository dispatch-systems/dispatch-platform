import { useEffect, useRef, useState, type PointerEvent } from 'react';
import { Plus, RotateCcw, X } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { Widget } from '../../../core/shell/frontend/runtime/slots.js';
import { ConfirmDialog, Header, Modal } from '../../../core/shell/frontend/ui/index.js';
import type { PlacedWidget } from '../api/index.js';
import { COLUMNS, GAP, ROWS } from './grid.js';
import { adding, keeping, moving, resized, shown, type Arrange, type Edge } from './layout.js';
import { WidgetFrame } from './WidgetFrame.js';

/** Each edge and corner of a widget, which resizing it drags. */
const EDGES: readonly Edge[] = ['n', 'e', 's', 'w', 'ne', 'se', 'sw', 'nw'];
/** How far the pointer goes before a press becomes a drag, in pixels. */
const SLOP = 4;

/** A widget held: where it is under the pointer, and the page as it would be dropped. */
type Held = { id: string; left: number; top: number; target: PlacedWidget; layout: PlacedWidget[] };

/** The size of the space the board is drawn in, as the window gives it, once it has one. */
function useSize() {
  const area = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<{ width: number; height: number }>();
  useEffect(() => {
    const element = area.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry!.contentRect;
      setSize({ width, height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return { area, size };
}

/**
 * The page while its member arranges it: each widget picked up and dropped anywhere on the
 * grid, those where it would land moving aside while it's held there and back once it moves on;
 * resized from any side, snapping to its nearest size as the pointer goes; or removed; others added from the list of those that fit. Every change shows at once and is
 * kept behind it.
 */
export function HomeEditor({
  view,
  available,
  placed,
  adding: open,
  arrange,
  reset,
  done,
}: {
  view: DspView;
  available: readonly Widget[];
  placed: readonly { widget: Widget; at: PlacedWidget }[];
  /** Whether it opens on the list of widgets to add. */
  adding: boolean;
  arrange: Arrange;
  reset: () => void;
  done: () => void;
}) {
  const { area, size } = useSize();
  const [listing, setListing] = useState(open);
  const [resetting, setResetting] = useState(false);
  const [held, setHeld] = useState<Held>();
  const [resizing, setResizing] = useState<PlacedWidget>();
  const [added, setAdded] = useState<string>();
  const origin = placed.map(({ at }) => at);
  // A cell and the gap after it, in pixels: the most a square one can be with the whole grid in
  // the space.
  const pitch = size ? Math.min((size.width + GAP) / COLUMNS, (size.height + GAP) / ROWS) : 0;
  const [across, down] = [pitch, pitch];
  const box = ({ x, y, w, h }: PlacedWidget) => ({
    transform: `translate(${x * across}px, ${y * down}px)`,
    width: w * across - GAP,
    height: h * down - GAP,
  });

  // The widgets shown, as each change makes them on what's kept: one placed elsewhere since
  // keeps its place.
  const showing = (layout: PlacedWidget[]) => shown(layout, available).map(({ at }) => at);
  /** Keeps each of `moved` where it is now. */
  const place = (moved: readonly PlacedWidget[]) =>
    arrange((layout) =>
      keeping(
        layout,
        available,
        showing(layout).map((at) => moved.find((each) => each.id === at.id) ?? at),
      ),
    );
  const remove = (id: string) =>
    arrange((layout) =>
      keeping(
        layout,
        available,
        showing(layout).filter((at) => at.id !== id),
      ),
    );
  const add = (widget: Widget) => {
    setAdded(widget.id);
    arrange((layout) => {
      const now = showing(layout);
      const at = !now.some((each) => each.id === widget.id) && adding(now, widget);
      return at ? keeping(layout, available, [...now, at]) : layout;
    });
  };

  // Follows the pointer from where it pressed: the widget under it, at the cells nearest, and
  // the page as dropping it there would leave it, kept as it's let go.
  const pickUp = (event: PointerEvent<HTMLElement>, at: PlacedWidget) => {
    if (event.button !== 0 || (event.target as Element).closest('.home-remove, .home-handle'))
      return;
    const cell = event.currentTarget;
    cell.setPointerCapture(event.pointerId);
    const [startX, startY] = [event.clientX, event.clientY];
    let now: Held | undefined;
    const move = (moved: globalThis.PointerEvent) => {
      const [dx, dy] = [moved.clientX - startX, moved.clientY - startY];
      if (!now && Math.hypot(dx, dy) < SLOP) return;
      const [left, top] = [at.x * across + dx, at.y * down + dy];
      const to = {
        x: Math.min(Math.max(Math.round(left / across), 0), COLUMNS - at.w),
        y: Math.min(Math.max(Math.round(top / down), 0), ROWS - at.h),
      };
      const stays = now && now.target.x === to.x && now.target.y === to.y;
      const layout = stays ? undefined : moving(origin, at.id, to);
      now = {
        id: at.id,
        left,
        top,
        target: layout ? { ...at, ...to } : (now?.target ?? at),
        layout: layout ?? now?.layout ?? origin,
      };
      setHeld(now);
    };
    const end = () => {
      cell.removeEventListener('pointermove', move);
      cell.removeEventListener('pointerup', end);
      cell.removeEventListener('pointercancel', end);
      if (now)
        place(
          now.layout.filter(
            (each, index) => each.x !== origin[index]!.x || each.y !== origin[index]!.y,
          ),
        );
      setHeld(undefined);
    };
    cell.addEventListener('pointermove', move);
    cell.addEventListener('pointerup', end);
    cell.addEventListener('pointercancel', end);
  };

  // Follows the pointer from the handle it pressed, the widget taking the size of its own nearest
  // as it goes, kept as it's let go.
  const resize = (
    event: PointerEvent<HTMLElement>,
    widget: Widget,
    at: PlacedWidget,
    edge: Edge,
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const [startX, startY] = [event.clientX, event.clientY];
    const others = origin.filter((each) => each.id !== at.id);
    let last = at;
    const move = (moved: globalThis.PointerEvent) => {
      const next = resized(
        widget,
        at,
        edge,
        [(moved.clientX - startX) / across, (moved.clientY - startY) / down],
        others,
      );
      if (!next || (['x', 'y', 'w', 'h'] as const).every((key) => next[key] === last[key])) return;
      last = next;
      setResizing(next);
    };
    const end = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', end);
      handle.removeEventListener('pointercancel', end);
      if (last !== at) place([last]);
      setResizing(undefined);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
  };

  const unplaced = available.filter(
    (widget) => !placed.some((each) => each.widget === widget) && adding(origin, widget),
  );
  return (
    <>
      <Header title="Home">
        <button disabled={!unplaced.length} onClick={() => setListing(true)}>
          <Plus size={16} />
          Add widget
        </button>
        <button onClick={() => setResetting(true)}>
          <RotateCcw size={16} />
          Reset
        </button>
        <button className="primary" onClick={done}>
          Done
        </button>
      </Header>
      <div ref={area} className="home-area">
        <div
          className={held ? 'home-board home-holding' : 'home-board'}
          style={{ width: COLUMNS * pitch - GAP, height: ROWS * pitch - GAP }}
        >
          {size && held && <div className="home-drop" style={box(held.target)} />}
          {size &&
            placed.map(({ widget, at }, index) => {
              const lifted = held?.id === widget.id;
              const now =
                held?.layout[index] ?? (resizing?.id === widget.id ? resizing : undefined) ?? at;
              const classes = [
                'home-cell',
                lifted && 'home-lifted',
                widget.id === added && 'home-added',
              ];
              return (
                <div
                  key={widget.id}
                  className={classes.filter(Boolean).join(' ')}
                  style={
                    lifted
                      ? { ...box(at), transform: `translate(${held.left}px, ${held.top}px)` }
                      : box(now)
                  }
                  onPointerDown={(event) => pickUp(event, at)}
                >
                  <WidgetFrame widget={widget} columns={now.w} rows={now.h} view={view} />
                  <button
                    type="button"
                    className="home-remove"
                    aria-label={`Remove ${widget.label}`}
                    onClick={() => remove(widget.id)}
                  >
                    <X size={14} aria-hidden="true" />
                  </button>
                  {EDGES.map((edge) => (
                    <span
                      key={edge}
                      className={`home-handle home-handle-${edge}`}
                      aria-hidden="true"
                      onPointerDown={(event) => resize(event, widget, at, edge)}
                    />
                  ))}
                </div>
              );
            })}
        </div>
      </div>
      {listing && (
        <Modal title="Add widget" onClose={() => setListing(false)}>
          <ul className="home-add">
            {unplaced.map((widget) => {
              const Icon = widget.icon;
              return (
                <li key={widget.id}>
                  <button
                    type="button"
                    onClick={() => {
                      add(widget);
                      setListing(false);
                    }}
                  >
                    <Icon size={18} aria-hidden="true" />
                    {widget.label}
                  </button>
                </li>
              );
            })}
          </ul>
        </Modal>
      )}
      {resetting && (
        <ConfirmDialog
          title="Reset Home"
          confirm="Reset"
          onCancel={() => setResetting(false)}
          onConfirm={() => {
            reset();
            setResetting(false);
          }}
        >
          Your widgets go back to where they start.
        </ConfirmDialog>
      )}
    </>
  );
}
