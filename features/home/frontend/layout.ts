import type { PlacedWidget } from '../api/index.js';
import { COLUMNS } from './grid.js';
import {
  widgetSizes,
  type Widget,
  type WidgetSize,
} from '../../../core/shell/frontend/runtime/slots.js';

// Where each widget sits on a member's Home page, in cells of its grid. What a member keeps is
// every widget they placed, those of features they don't have now among them; what the page
// shows is those they have, each at the step it was placed at.

/** A change to the widgets a member placed, made on those kept when it's saved. */
export type Arrange = (update: (placed: PlacedWidget[]) => PlacedWidget[]) => void;

type Cells = Pick<PlacedWidget, 'x' | 'y' | 'w' | 'h'>;
const overlaps = (a: Cells, b: Cells) =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/** The first spot, row by row from the top left, where `w` by `h` cells overlap none `placed`. */
export function free(placed: readonly Cells[], w: number, h: number) {
  for (let y = 0; ; y++)
    for (let x = 0; x + w <= COLUMNS; x++)
      if (!placed.some((each) => overlaps(each, { x, y, w, h }))) return { x, y };
}

/** The widget `id` names, as it is known now or once was. */
export const widgetOf = (widgets: readonly Widget[], id: string) =>
  widgets.find((widget) => widget.id === id || widget.formerly?.includes(id));

/** `widget` placed at its first step, where it first fits beside `placed`. */
export function adding(placed: readonly Cells[], widget: Widget): PlacedWidget {
  const { columns: w, rows: h } = widgetSizes[widget.sizes[0]];
  return { id: widget.id, ...free(placed, w, h), w, h };
}

/**
 * The layout of a member who never arranged theirs: each widget with a place on it (`home`), in
 * that order, filling the grid from the top left.
 */
export const starting = (widgets: readonly Widget[]) =>
  widgets
    .filter((widget) => widget.home !== undefined)
    .sort((a, b) => a.home! - b.home!)
    .reduce<PlacedWidget[]>((placed, widget) => [...placed, adding(placed, widget)], []);

/**
 * What the page shows of `layout`: each widget placed that the member has now, by its id now,
 * at the step of its own nearest the cells it was placed in. One back from a feature switched
 * off, where another was placed meanwhile, shows where it first fits, and is kept there once
 * they arrange the page.
 */
export function shown(layout: readonly PlacedWidget[], widgets: readonly Widget[]) {
  const placed: { widget: Widget; size: WidgetSize; at: PlacedWidget }[] = [];
  for (const each of layout) {
    const widget = widgetOf(widgets, each.id);
    if (!widget || placed.some((other) => other.widget === widget)) continue;
    const size = nearest(widget, each.w, each.h);
    const { columns: w, rows: h } = widgetSizes[size];
    const cells = { x: each.x, y: each.y, w, h };
    const others = placed.map((other) => other.at);
    const at = others.some((other) => overlaps(other, cells)) ? free(others, w, h) : cells;
    placed.push({ widget, size, at: { id: widget.id, ...cells, ...at } });
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
export const nearest = (widget: Widget, w: number, h: number) =>
  widget.sizes.reduce((best, step) => {
    const off = (each: WidgetSize) =>
      Math.abs(widgetSizes[each].columns - w) + Math.abs(widgetSizes[each].rows - h);
    return off(step) < off(best) ? step : best;
  });
