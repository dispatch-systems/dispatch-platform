import type { PlacedWidget } from '../api/index.js';
import { COLUMNS, ROWS } from './grid.js';
import {
  widgetSizes,
  type Widget,
  type WidgetSize,
} from '../../../core/shell/frontend/runtime/slots.js';

// Where each widget sits on a member's Home page, in cells of its grid, which the page fills
// without scrolling. What a member keeps is every widget they placed, those of features they
// don't have now among them; what the page shows is those they have, each at the step it was
// placed at.

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

/**
 * The free spot for `cells` nearest where they are, beside `placed`, the first row by row from
 * the top left among those as near; none where the grid has none.
 */
function nearest(placed: readonly Cells[], cells: Cells) {
  let best: { x: number; y: number } | undefined;
  let off = Infinity;
  for (let y = 0; y + cells.h <= ROWS; y++)
    for (let x = 0; x + cells.w <= COLUMNS; x++) {
      const distance = Math.abs(x - cells.x) + Math.abs(y - cells.y);
      if (distance < off && fits(placed, { ...cells, x, y })) [best, off] = [{ x, y }, distance];
    }
  return best;
}

/** The widget `id` names, as it is known now or once was. */
export const widgetOf = (widgets: readonly Widget[], id: string) =>
  widgets.find((widget) => widget.id === id || widget.formerly?.includes(id));

/** `widget` at its first step, where it first fits beside `placed`; none where it can't. */
export function adding(placed: readonly Cells[], widget: Widget): PlacedWidget | undefined {
  const { columns: w, rows: h } = widgetSizes[widget.sizes[0]];
  const spot = nearest(placed, { x: 0, y: 0, w, h });
  return spot && { id: widget.id, ...spot, w, h };
}

/**
 * The layout of a member who never arranged theirs: each widget with a place on it (`home`), in
 * that order, filling the grid from the top left while it fits.
 */
export const starting = (widgets: readonly Widget[]) =>
  widgets
    .filter((widget) => widget.home !== undefined)
    .sort((a, b) => a.home! - b.home!)
    .reduce<PlacedWidget[]>((placed, widget) => {
      const added = adding(placed, widget);
      return added ? [...placed, added] : placed;
    }, []);

/**
 * What the page shows of `layout`: each widget placed that the member has now, by its id now,
 * at the step of its own nearest the cells it was placed in. One back from a feature switched
 * off, where another was placed meanwhile, shows at the free spot nearest its own, and is kept
 * there once they arrange the page; with none free, it waits, kept, until there is.
 */
export function shown(layout: readonly PlacedWidget[], widgets: readonly Widget[]) {
  const placed: { widget: Widget; size: WidgetSize; at: PlacedWidget }[] = [];
  for (const each of layout) {
    const widget = widgetOf(widgets, each.id);
    if (!widget || placed.some((other) => other.widget === widget)) continue;
    const size = stepNearest(widget, each.w, each.h);
    const { columns: w, rows: h } = widgetSizes[size];
    const others = placed.map((other) => other.at);
    const spot = nearest(others, { x: each.x, y: each.y, w, h });
    if (spot) placed.push({ widget, size, at: { id: widget.id, ...spot, w, h } });
  }
  return placed;
}

/**
 * `layout` with the widgets the page shows as `placed`, each kept by its id now, and those of
 * features the member doesn't have now kept where they were.
 */
export const keeping = (
  layout: readonly PlacedWidget[],
  widgets: readonly Widget[],
  placed: readonly PlacedWidget[],
) => [...placed, ...layout.filter((each) => !widgetOf(widgets, each.id))];

/** `widget`'s step nearest `w` by `h` cells. */
export const stepNearest = (widget: Widget, w: number, h: number) =>
  widget.sizes.reduce((best, step) => {
    const off = (each: WidgetSize) =>
      Math.abs(widgetSizes[each].columns - w) + Math.abs(widgetSizes[each].rows - h);
    return off(step) < off(best) ? step : best;
  });

/**
 * `placed` with `id` moved to `to`, each widget it would cover moved to the free spot nearest its
 * own; none where one has nowhere to go. Worked out from where every widget was as the drag
 * began, so moving on puts each back.
 */
export function moving(
  placed: readonly PlacedWidget[],
  id: string,
  to: { x: number; y: number },
): PlacedWidget[] | undefined {
  const held = placed.find((each) => each.id === id);
  if (!held) return undefined;
  const at = { ...held, ...to };
  if (!fits([], at)) return undefined;
  const covered = (each: PlacedWidget) => each.id !== id && overlaps(each, at);
  const settled = [at, ...placed.filter((each) => each.id !== id && !covered(each))];
  for (const each of placed.filter(covered)) {
    const spot = nearest(settled, each);
    if (!spot) return undefined;
    settled.push({ ...each, ...spot });
  }
  return placed.map((each) => settled.find((other) => other.id === each.id)!);
}

/** The sides of a widget a resize handle moves: an edge, or the two of a corner. */
export type Edge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

/**
 * `at` with `edge` dragged `columns` across and `rows` down, in cells and parts of one, at
 * `widget`'s step nearest that, its other sides where they were; none where that would leave
 * the grid or cover any of `others`.
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
  const size = widgetSizes[stepNearest(widget, w, h)];
  const next = {
    ...at,
    x: edge.includes('w') ? at.x + at.w - size.columns : at.x,
    y: edge.includes('n') ? at.y + at.h - size.rows : at.y,
    w: size.columns,
    h: size.rows,
  };
  return fits(others, next) ? next : undefined;
}
