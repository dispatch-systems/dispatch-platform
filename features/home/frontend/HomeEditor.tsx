import { useState } from 'react';
import ReactGridLayout, {
  getCompactor,
  useContainerWidth,
  type LayoutItem,
} from 'react-grid-layout';
import 'react-grid-layout/css/styles.css';
import 'react-resizable/css/styles.css';
import { Plus, RotateCcw, X } from 'lucide-react';
import type { DspView } from '../../../core/accounts/api/index.js';
import {
  widgetSizes,
  type Widget,
  type WidgetSize,
} from '../../../core/shell/frontend/runtime/slots.js';
import { ConfirmDialog, Header, Modal } from '../../../core/shell/frontend/ui/index.js';
import type { PlacedWidget } from '../api/index.js';
import { COLUMNS, GAP } from './grid.js';
import { adding, keeping, nearest, shown, type Arrange } from './layout.js';
import { WidgetFrame } from './WidgetFrame.js';

/** Placed anywhere, never over another: one dragged onto another goes back. */
const free = getCompactor(null, false, true);

/** Resizing `widget` stops at its steps alone. */
const steps = (widget: Widget): NonNullable<LayoutItem['constraints']>[number] => ({
  name: 'steps',
  constrainSize: (_item, w, h) => {
    const { columns, rows } = widgetSizes[nearest(widget, w, h)];
    return { w: columns, h: rows };
  },
});

/**
 * The page while its member arranges it: each widget dragged anywhere on the grid, resized
 * between its steps, or removed; others added from the list of those they may add. Every change
 * shows at once and is kept behind it.
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
  // The widgets shown, as each change makes them on what's kept: one placed elsewhere since
  // keeps its place.
  const showing = (layout: PlacedWidget[]) => shown(layout, available).map(({ at }) => at);
  const place = ({ i, x, y, w, h }: LayoutItem) =>
    arrange((layout) =>
      keeping(
        layout,
        available,
        showing(layout).map((at) => (at.id === i ? { ...at, x, y, w, h } : at)),
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
  const add = (widget: Widget) =>
    arrange((layout) => {
      const now = showing(layout);
      return now.some((at) => at.id === widget.id)
        ? layout
        : keeping(layout, available, [...now, adding(now, widget)]);
    });
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
            layout={placed.map(({ widget, at }) => ({
              ...at,
              i: at.id,
              constraints: [steps(widget)],
            }))}
            gridConfig={{
              cols: COLUMNS,
              rowHeight: (width - GAP * (COLUMNS - 1)) / COLUMNS,
              margin: [GAP, GAP],
              containerPadding: [0, 0],
            }}
            dragConfig={{ cancel: '.home-remove' }}
            compactor={free}
            onDragStop={(_layout, _from, to) => to && place(to)}
            onResizeStop={(_layout, _from, to) => to && place(to)}
          >
            {placed.map(({ widget, size }) => (
              <div key={widget.id} className="home-cell">
                <WidgetFrame widget={widget} size={size} view={view} />
                <button
                  type="button"
                  className="home-remove"
                  aria-label={`Remove ${widget.label}`}
                  onClick={() => remove(widget.id)}
                >
                  <X size={14} aria-hidden="true" />
                </button>
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
