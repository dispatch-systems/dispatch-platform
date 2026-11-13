import test from 'node:test';
import assert from 'node:assert/strict';
import type { Widget } from '../../../../core/shell/frontend/runtime/slots.js';
import { adding, keeping, moving, resized, shown, starting } from '../../frontend/layout.js';

const widget = (id: string, more: Partial<Widget> = {}) =>
  ({ id, label: id, sizes: ['small', 'wide'], load: async () => () => null, ...more }) as Widget;
const at = (id: string, x: number, y: number, w = 3, h = 3) => ({ id, x, y, w, h });

test('a page never arranged starts with the widgets that have a place on it, in that order, from the top left', () => {
  const widgets = [
    widget('vans.fleet', { home: 2, sizes: ['wide'] }),
    widget('dvic.today', { home: 1 }),
    widget('team.today'),
    widget('routes.today', { home: 3, sizes: ['full'] }),
  ];
  assert.deepEqual(starting(widgets), [
    at('dvic.today', 0, 0),
    at('vans.fleet', 3, 0, 6, 3),
    at('routes.today', 0, 3, 12, 6),
  ]);
});

test("the page shows the widgets the member has, by their ids now, and keeps the rest's places", () => {
  const widgets = [widget('vans.fleet', { formerly: ['vans.overview'] }), widget('dvic.today')];
  const layout = [
    at('vans.overview', 0, 0),
    // Back from a feature switched off, where DVIC was placed meanwhile.
    at('dvic.today', 0, 0),
    // Of a feature the member doesn't have now.
    at('team.today', 6, 0),
    // Placed at cells that are none of its steps: shown at its nearest.
    { ...at('timecard.today', 0, 9), w: 5, h: 4 },
  ];
  const placed = shown(layout, [...widgets, widget('timecard.today')]);
  assert.deepEqual(
    placed.map(({ at: each, size }) => [each, size]),
    [
      [at('vans.fleet', 0, 0), 'small'],
      [at('dvic.today', 3, 0), 'small'],
      [at('timecard.today', 0, 9, 6, 3), 'wide'],
    ],
  );
  assert.deepEqual(keeping(layout, widgets, [at('vans.fleet', 0, 3)]), [
    at('vans.fleet', 0, 3),
    at('team.today', 6, 0),
    { ...at('timecard.today', 0, 9), w: 5, h: 4 },
  ]);
});

test('a widget resized from any side takes its nearest step, its other sides staying, and never leaves the grid or covers another', () => {
  const vans = widget('vans.fleet', { sizes: ['small', 'wide', 'large'] });
  const placed = at('vans.fleet', 3, 3);
  // A cell and a half to the right is nearer wide than small.
  assert.deepEqual(resized(vans, placed, 'e', [1.6, 0], []), at('vans.fleet', 3, 3, 6, 3));
  assert.deepEqual(resized(vans, placed, 'e', [1.4, 0], []), placed);
  // From the left, it grows leftward; from the top-left corner, up and left.
  assert.deepEqual(resized(vans, placed, 'w', [-3, 0], []), at('vans.fleet', 0, 3, 6, 3));
  assert.deepEqual(resized(vans, placed, 'nw', [-3, -3], []), at('vans.fleet', 0, 0, 6, 6));
  // Past the grid's edge, or over another, it doesn't.
  assert.equal(resized(vans, at('vans.fleet', 9, 0), 'e', [3, 0], []), undefined);
  assert.equal(resized(vans, placed, 'w', [-3, 0], [at('dvic.today', 0, 3)]), undefined);
});

test("a widget held over others moves them to the free cells nearest their own, and they're back once it moves on", () => {
  const placed = [at('dvic.today', 0, 0), at('vans.fleet', 3, 0), at('team.today', 0, 3)];
  // Over Vans, Vans makes way to the nearest free cells; DVIC's cells are free once it's held.
  assert.deepEqual(moving(placed, 'dvic.today', { x: 3, y: 0 }), [
    at('dvic.today', 3, 0),
    at('vans.fleet', 0, 0),
    at('team.today', 0, 3),
  ]);
  // Moved on from there, every other widget is where it was before it was picked up.
  assert.deepEqual(moving(placed, 'dvic.today', { x: 9, y: 9 }), [
    at('dvic.today', 9, 9),
    at('vans.fleet', 3, 0),
    at('team.today', 0, 3),
  ]);
  // Off the grid, it goes nowhere.
  assert.equal(moving(placed, 'dvic.today', { x: 10, y: 0 }), undefined);
});

test('the grid holds only what fits the page: a widget with no room is not added', () => {
  const full = [{ ...at('routes.today', 0, 0), w: 12, h: 12 }];
  assert.equal(adding(full, widget('vans.fleet')), undefined);
  assert.deepEqual(adding([at('dvic.today', 0, 0)], widget('vans.fleet')), at('vans.fleet', 3, 0));
});
