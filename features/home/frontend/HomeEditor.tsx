import { useState, type PointerEvent } from 'react';
import ReactGridLayout, { getCompactor, useContainerWidth } from 'react-grid-layout';
import 'react-grid-layout/css/styles.css';
import { Plus, RotateCcw, X } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import type { Widget, WidgetSize } from '../../../core/shell/frontend/runtime/slots.js';
import { ConfirmDialog, Header, Modal } from '../../../core/shell/frontend/ui/index.js';
import type { PlacedWidget } from '../api/index.js';
import { COLUMNS, GAP } from './grid.js';
import { adding, keeping, resized, shown, type Arrange, type Edge } from './layout.js';
import { WidgetFrame } from './WidgetFrame.js';

/** Placed anywhere, never over another: one dragged onto another goes back. */
const free = getCompactor(null, false, true);
/** Each edge and corner of a widget, which resizing it drags. */
const EDGES: readonly Edge[] = ['n', 'e', 's', 'w', 'ne', 'se', 'sw', 'nw'];

/**
 * The page while its member arranges it: each widget dragged anywhere on the grid, resized from
 * any side, snapping between its steps as it goes, or removed; others added from the list of
 * those they may add. Every change shows at once and is kept behind it.
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
  placed: readonly { widget: Widget; size: WidgetSize; at: PlacedWidget }[];
  /** Whether it opens on the list of widgets to add. */
  adding: boolean;
  arrange: Arrange;
  reset: () => void;
  done: () => void;
}) {
  const { width, containerRef, mounted } = useContainerWidth();
  const [listing, setListing] = useState(open);
  const [resetting, setResetting] = useState(false);
  // The widget being resized, where it is now, and the one just added, which eases in.
  const [resizing, setResizing] = useState<PlacedWidget>();
  const [added, setAdded] = useState<string>();
  // The widgets shown, as each change makes them on what's kept: one placed elsewhere since
  // keeps its place.
  const showing = (layout: PlacedWidget[]) => shown(layout, available).map(({ at }) => at);
  const place = (moved: PlacedWidget) =>
    arrange((layout) =>
      keeping(
        layout,
        available,
        showing(layout).map((at) => (at.id === moved.id ? { ...at, ...moved } : at)),
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
      return now.some((at) => at.id === widget.id)
        ? layout
        : keeping(layout, available, [...now, adding(now, widget)]);
    });
  };
  // A cell and the gap after it, in pixels.
  const cell = (width + GAP) / COLUMNS;
  // Resizing follows the pointer from the handle it began on, the widget taking its nearest
  // step as it goes, and is kept as it ends.
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
    const others = placed.filter((each) => each.widget !== widget).map((each) => each.at);
    let last = at;
    const move = (moved: globalThis.PointerEvent) => {
      const next = resized(
        widget,
        at,
        edge,
        [(moved.clientX - startX) / cell, (moved.clientY - startY) / cell],
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
      if (last !== at) place(last);
      setResizing(undefined);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
  };
  const unplaced = available.filter((widget) => !placed.some((each) => each.widget === widget));
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
      <div ref={containerRef} className="home-editing">
        {mounted && (
          <ReactGridLayout
            width={width}
            layout={placed.map(({ at }) => {
              const now = resizing?.id === at.id ? resizing : at;
              return { ...now, i: now.id };
            })}
            gridConfig={{
              cols: COLUMNS,
              rowHeight: cell - GAP,
              margin: [GAP, GAP],
              containerPadding: [0, 0],
            }}
            dragConfig={{ cancel: '.home-remove, .home-handle' }}
            resizeConfig={{ enabled: false }}
            compactor={free}
            onDragStop={(_layout, _from, to) =>
              to && place({ id: to.i, x: to.x, y: to.y, w: to.w, h: to.h })
            }
          >
            {placed.map(({ widget, size, at }) => (
              <div
                key={widget.id}
                className={widget.id === added ? 'home-cell home-added' : 'home-cell'}
              >
                <WidgetFrame widget={widget} size={size} view={view} />
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
            ))}
          </ReactGridLayout>
        )}
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
