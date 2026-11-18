import type { PlacedWidget } from '../api/index.js';
import { COLUMNS, ROWS } from './grid.js';
import type { Widget, WidgetCells } from '../../../core/shell/frontend/runtime/slots.js';

// Where each widget sits on a member's Home page, in cells of its grid, which the page fills
// without scrolling. What a member keeps is every widget they placed, those of features they
// don't have now among them; what the page shows is those they have, each at the step it was
// placed in.

/** A change to the widgets a member placed, made on those kept when it's saved. */
export type Arrange = (update: (placed: PlacedWidget[]) => PlacedWidget[]) => void;

type Cells = Pick<PlacedWidget, 'x' | 'y' | 'w' | 'h'>;
const overlaps = (a: Cells, b: Cells) =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
/** Whether `cells` are on the grid, over none of `placed`. */
const fits = (placed: readonly Cells[], cells: Cells) =>
  cells.x >= 0 &&
  cells.y >= 0 &&
  cells.x + cells.w <= COLUMNS &&
  cells.y + cells.h <= ROWS &&
  !placed.some((each) => overlaps(each, cells));

/** How many cells `a` and `b` share. */
const shared = (a: Cells, b: Cells) =>
  Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)) *
  Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));

/**
 * The free spot for `cells` nearest where they are, beside `placed`: among those as near, one on
 * the cells `left` free, then the first row by row from the top left; none where the grid has
 * none.
 */
function nearest(placed: readonly Cells[], cells: Cells, left?: Cells) {
  let best: { x: number; y: number } | undefined;
  let off = Infinity;
  for (let y = 0; y + cells.h <= ROWS; y++)
    for (let x = 0; x + cells.w <= COLUMNS; x++) {
      const spot = { ...cells, x, y };
      // Half a cell nearer than another as far, so never nearer than one a cell closer.
      const distance =
        Math.abs(x - cells.x) + Math.abs(y - cells.y) - (left && overlaps(spot, left) ? 0.5 : 0);
      if (distance < off && fits(placed, spot)) [best, off] = [{ x, y }, distance];
    }
  return best;
}

/**
 * Every one of `sizes` on grids of their own, as few as hold them, largest first: each where it
 * first fits, nearest the top left of the first grid with room.
 */
export function sizePages(sizes: readonly WidgetCells[]) {
  const pages: Cells[][] = [];
  const largest = [...sizes].sort((a, b) => b.rows - a.rows || b.columns - a.columns);
  for (const { columns: w, rows: h } of largest) {
    const cells = { x: 0, y: 0, w, h };
    const page = pages.find((placed) => nearest(placed, cells)) ?? (pages.push([]), pages.at(-1)!);
    page.push({ ...cells, ...nearest(page, cells)! });
  }
  return pages;
}

/** The widget `id` names, as it is known now or once was. */
export const widgetOf = (widgets: readonly Widget[], id: string) =>
  widgets.find((widget) => widget.id === id || widget.formerly?.includes(id));

/** `widget` at the size it's added at, where it first fits beside `placed`; none where it can't. */
export function adding(placed: readonly Cells[], widget: Widget): PlacedWidget | undefined {
  const { columns: w, rows: h } = widget.sizes[0];
  const spot = nearest(placed, { x: 0, y: 0, w, h });
  return spot && { id: widget.id, ...spot, w, h };
}

/**
 * What the page shows of `layout`: each widget placed that the member has now, by its id now,
 * where it was placed, at the size of its own nearest the cells it was placed in. One back from a
 * feature switched off, where another was placed meanwhile, shows at the free cells nearest its
 * own, and is kept there once they arrange the page; with none free, it waits, kept, until there
 * are.
 */
export function shown(layout: readonly PlacedWidget[], widgets: readonly Widget[]) {
  const placed: { widget: Widget; at: PlacedWidget }[] = [];
  for (const each of layout) {
    const widget = widgetOf(widgets, each.id);
    if (!widget || placed.some((other) => other.widget === widget)) continue;
    const { columns: w, rows: h } = sizeNearest(widget, each.w, each.h);
    const spot = nearest(
      placed.map((other) => other.at),
      { x: each.x, y: each.y, w, h },
    );
    if (spot) placed.push({ widget, at: { id: widget.id, ...spot, w, h } });
  }
  return placed;
}

/**
 * `layout` with the widgets the page shows as `placed`, each kept by its id now, and every other
 * it kept where it was: one of a feature the member doesn't have now, and one waiting for room;
 * none of `removed`.
 */
export function keeping(
  layout: readonly PlacedWidget[],
  widgets: readonly Widget[],
  placed: readonly PlacedWidget[],
  removed?: string,
) {
  const idNow = (each: PlacedWidget) => widgetOf(widgets, each.id)?.id ?? each.id;
  const waiting = layout.filter(
    (each) => idNow(each) !== removed && !placed.some((at) => at.id === idNow(each)),
  );
  return [...placed, ...waiting];
}

/** `placed` with `at` in its own entry's place; none where it would leave the grid or cover another. */
export const setting = (placed: readonly PlacedWidget[], at: PlacedWidget) =>
  fits(
    placed.filter((each) => each.id !== at.id),
    at,
  )
    ? placed.map((each) => (each.id === at.id ? at : each))
    : undefined;

/**
 * `placed` with `id` moved to `to`. A widget makes way once more than half of it, or of the one
 * moved where that is the smaller, is covered: to the free spot nearest its own, the cells the
 * moved one left first among those as near. One covered less stays, and the moved one lands on
 * the free cells nearest `to`, beside it. None where a widget has nowhere to go. Worked out from
 * where every widget was as the drag began, so moving on puts each back.
 */
export function moving(
  placed: readonly PlacedWidget[],
  id: string,
  to: { x: number; y: number },
): PlacedWidget[] | undefined {
  const held = placed.find((each) => each.id === id);
  if (!held) return undefined;
  const over = { ...held, ...to };
  if (!fits([], over)) return undefined;
  const others = placed.filter((each) => each.id !== id);
  const makesWay = (each: PlacedWidget) =>
    shared(each, over) * 2 > Math.min(each.w * each.h, held.w * held.h);
  const staying = others.filter((each) => !makesWay(each));
  const spot = nearest(staying, over);
  if (!spot) return undefined;
  const settled = [{ ...held, ...spot }, ...staying];
  for (const each of others.filter(makesWay)) {
    const free = nearest(settled, each, held);
    if (!free) return undefined;
    settled.push({ ...each, ...free });
  }
  return placed.map((each) => settled.find((other) => other.id === each.id)!);
}

/** The sides of a widget a resize handle moves: an edge, or the two of a corner. */
export type Edge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

/** The size of `sizes` nearest `w` by `h` cells, the first of those as near; none of none. */
const nearestOf = (sizes: readonly WidgetCells[], w: number, h: number) => {
  const off = (each: WidgetCells) => Math.abs(each.columns - w) + Math.abs(each.rows - h);
  return sizes.reduce<WidgetCells | undefined>(
    (best, size) => (best && off(best) <= off(size) ? best : size),
    undefined,
  );
};

/** `widget`'s size nearest `w` by `h` cells, the first of those as near. */
export const sizeNearest = (widget: Widget, w: number, h: number) => nearestOf(widget.sizes, w, h)!;

/**
 * `at` with `edge` dragged `columns` across and `rows` down, in cells and parts of one, its other
 * sides where they were: at the size nearest that among `widget`'s that stay on the grid and
 * cover none of `others`, so it grows as far as it has room however far the pointer goes; none
 * where no size of its fits.
 */
export function resized(
  widget: Widget,
  at: PlacedWidget,
  edge: Edge,
  [columns, rows]: readonly [number, number],
  others: readonly Cells[],
): PlacedWidget | undefined {
  const w = at.w + (edge.includes('e') ? columns : edge.includes('w') ? -columns : 0);
  const h = at.h + (edge.includes('s') ? rows : edge.includes('n') ? -rows : 0);
  const sized = (size: WidgetCells): PlacedWidget => ({
    ...at,
    x: edge.includes('w') ? at.x + at.w - size.columns : at.x,
    y: edge.includes('n') ? at.y + at.h - size.rows : at.y,
    w: size.columns,
    h: size.rows,
  });
  const size = nearestOf(
    widget.sizes.filter((each) => fits(others, sized(each))),
    w,
    h,
  );
  return size && sized(size);
}
