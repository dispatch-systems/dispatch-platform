import test from 'node:test';
import assert from 'node:assert/strict';
import type { Widget } from '../../../../core/shell/frontend/runtime/slots.js';
import { adding, keeping, moving, resized, setting, shown } from '../../frontend/layout.js';

const cells = (columns: number, rows: number) => ({ columns, rows });
const widget = (id: string, more: Partial<Widget> = {}) =>
  ({
    id,
    label: id,
    sizes: [cells(3, 3)],
    load: async () => () => null,
    ...more,
  }) as Widget;
const at = (id: string, x: number, y: number, w = 3, h = 3) => ({ id, x, y, w, h });

test("the page shows the widgets the member has, by their ids now, and keeps the rest's places", () => {
  const widgets = [widget('vans.fleet', { formerly: ['vans.overview'] }), widget('dvic.today')];
  const layout = [
    at('vans.overview', 0, 0),
    // Back from a feature switched off, where DVIC was placed meanwhile.
    at('dvic.today', 0, 0),
    // Of a feature the member doesn't have now.
    at('team.today', 6, 0),
  ];
  assert.deepEqual(
    shown(layout, widgets).map((each) => each.at),
    [at('vans.fleet', 0, 0), at('dvic.today', 3, 0)],
  );
  // Vans moved, DVIC and Team kept where they were: what the page shows of each is the caller's.
  assert.deepEqual(keeping(layout, widgets, [at('vans.fleet', 0, 3)]), [
    at('vans.fleet', 0, 3),
    at('dvic.today', 0, 0),
    at('team.today', 6, 0),
  ]);
});

test('a widget resized from any side snaps to the nearest of its own sizes, its other sides staying, and never leaves the grid or covers another', () => {
  const misses = widget('dvic.misses', { sizes: [cells(1, 1), cells(2, 1), cells(3, 2)] });
  const placed = at('dvic.misses', 3, 3, 1, 1);
  // A cell and a bit to the right is 2×1; on down and right, 3×2. Never a size it doesn't have.
  assert.deepEqual(resized(misses, placed, 'e', [1.2, 0], []), at('dvic.misses', 3, 3, 2, 1));
  assert.deepEqual(resized(misses, placed, 'se', [2, 1], []), at('dvic.misses', 3, 3, 3, 2));
  assert.deepEqual(resized(misses, placed, 'se', [6, 6], []), at('dvic.misses', 3, 3, 3, 2));
  assert.deepEqual(resized(misses, placed, 'e', [0.4, 0], []), placed);
  // From the top-left corner, it grows up and left.
  assert.deepEqual(resized(misses, placed, 'nw', [-2, -1], []), at('dvic.misses', 1, 2, 3, 2));
  // Past the grid's edge, or over another, it doesn't.
  assert.equal(resized(misses, at('dvic.misses', 11, 0, 1, 1), 'e', [2, 0], []), undefined);
  assert.equal(resized(misses, placed, 'e', [1, 0], [at('vans.fleet', 4, 3)]), undefined);
  // Placed at cells none of its sizes is, it shows at its nearest.
  assert.deepEqual(
    shown([at('dvic.misses', 0, 0, 3, 3)], [misses]).map((each) => each.at),
    [at('dvic.misses', 0, 0, 3, 2)],
  );
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
  assert.deepEqual(moving(placed, 'dvic.today', { x: 9, y: 4 }), [
    at('dvic.today', 9, 4),
    at('vans.fleet', 3, 0),
    at('team.today', 0, 3),
  ]);
  // Off the grid, it goes nowhere.
  assert.equal(moving(placed, 'dvic.today', { x: 10, y: 0 }), undefined);
});

test('the grid holds only what fits the page: a widget with no room is not added', () => {
  const full = [{ ...at('routes.today', 0, 0), w: 12, h: 7 }];
  assert.equal(adding(full, widget('vans.fleet')), undefined);
  assert.deepEqual(adding([at('dvic.today', 0, 0)], widget('vans.fleet')), at('vans.fleet', 3, 0));
});

test('a widget waiting for room is kept through every change but its own removal', () => {
  const whole = { sizes: [cells(12, 7)] as Widget['sizes'] };
  const widgets = [widget('dvic.today', whole), widget('vans.fleet', whole)];
  // Back from a feature switched off where Vans was placed meanwhile, filling the grid.
  const layout = [
    { ...at('vans.fleet', 0, 0), w: 12, h: 7 },
    { ...at('dvic.today', 0, 0), w: 12, h: 7 },
  ];
  assert.deepEqual(
    shown(layout, widgets).map((each) => each.at.id),
    ['vans.fleet'],
  );
  // Removing Vans keeps DVIC, which shows once there's room; removing DVIC itself drops it.
  assert.deepEqual(keeping(layout, widgets, [], 'vans.fleet'), [layout[1]]);
  assert.deepEqual(keeping(layout, widgets, [layout[0]!], 'dvic.today'), [layout[0]]);
  // A move of Vans keeps DVIC waiting.
  assert.deepEqual(keeping(layout, widgets, [layout[0]!]), layout);
});

test('a size made on one screen is kept on what another kept meanwhile only where it still fits', () => {
  const shownNow = [at('dvic.today', 0, 0), at('vans.fleet', 3, 0)];
  assert.deepEqual(setting(shownNow, at('dvic.today', 0, 0, 3, 6)), [
    at('dvic.today', 0, 0, 3, 6),
    at('vans.fleet', 3, 0),
  ]);
  assert.equal(setting(shownNow, at('dvic.today', 0, 0, 6, 3)), undefined);
});
