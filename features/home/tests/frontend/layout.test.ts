import test from 'node:test';
import assert from 'node:assert/strict';
import type { Widget } from '../../../../core/shell/frontend/runtime/slots.js';
import {
  adding,
  keeping,
  moving,
  resized,
  setting,
  shown,
  sizePages,
} from '../../frontend/layout.js';
import { sizesUpTo } from '../../../../core/shell/frontend/runtime/slots.js';

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

test('a widget resized from any side snaps to the nearest of its own sizes that has room, its other sides staying, and never leaves the grid or covers another', () => {
  const misses = widget('dvic.misses', { sizes: [cells(1, 1), cells(2, 1), cells(3, 2)] });
  const placed = at('dvic.misses', 3, 3, 1, 1);
  // A cell and a bit to the right is 2×1; on down and right, 3×2. Never a size it doesn't have.
  assert.deepEqual(resized(misses, placed, 'e', [1.2, 0], []), at('dvic.misses', 3, 3, 2, 1));
  assert.deepEqual(resized(misses, placed, 'se', [2, 1], []), at('dvic.misses', 3, 3, 3, 2));
  assert.deepEqual(resized(misses, placed, 'se', [6, 6], []), at('dvic.misses', 3, 3, 3, 2));
  assert.deepEqual(resized(misses, placed, 'e', [0.4, 0], []), placed);
  // From the top-left corner, it grows up and left.
  assert.deepEqual(resized(misses, placed, 'nw', [-2, -1], []), at('dvic.misses', 1, 2, 3, 2));
  // Past the grid's edge, or over another, it grows as far as it has room, however far and fast
  // the pointer went: here, not at all.
  const edge = at('dvic.misses', 11, 0, 1, 1);
  assert.deepEqual(resized(misses, edge, 'e', [2, 0], []), edge);
  assert.deepEqual(resized(misses, placed, 'e', [1, 0], [at('vans.fleet', 4, 3)]), placed);
  // With a column free before the next widget, it takes that column and no more.
  const any = widget('dvic.today', {
    sizes: [1, 2, 3, 4].flatMap((columns) => [1, 2, 3, 4].map((rows) => cells(columns, rows))) as [
      ReturnType<typeof cells>,
    ],
  });
  const beside = [at('vans.fleet', 7, 0, 2, 4)];
  assert.deepEqual(
    resized(any, at('dvic.today', 4, 0, 2, 4), 'e', [2.6, 0], beside),
    at('dvic.today', 4, 0, 3, 4),
  );
  // From a corner, the side with room still follows the pointer where the other has none.
  assert.deepEqual(
    resized(any, at('dvic.today', 5, 0, 2, 2), 'se', [1, 2], beside),
    at('dvic.today', 5, 0, 2, 4),
  );
  // Placed at cells none of its sizes is, it shows at its nearest.
  assert.deepEqual(
    shown([at('dvic.misses', 0, 0, 3, 3)], [misses]).map((each) => each.at),
    [at('dvic.misses', 0, 0, 3, 2)],
  );
});

test("a widget held over most of another moves it to the free cells nearest its own, and it's back once the held one moves on; over less of it, the held one lands beside it", () => {
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
  // Over half of Vans or less, Vans stays: DVIC lands on the free cells nearest, its own or
  // those past Vans.
  assert.deepEqual(moving(placed, 'dvic.today', { x: 1, y: 0 }), placed);
  assert.deepEqual(moving(placed, 'dvic.today', { x: 5, y: 0 }), [
    at('dvic.today', 6, 0),
    at('vans.fleet', 3, 0),
    at('team.today', 0, 3),
  ]);
  // A small widget dropped inside a large one moves it, though it covers little of it.
  const large = [at('dvic.today', 0, 0, 1, 1), at('vans.fleet', 4, 0, 4, 4)];
  assert.deepEqual(moving(large, 'dvic.today', { x: 5, y: 1 }), [
    at('dvic.today', 5, 1, 1, 1),
    at('vans.fleet', 6, 0, 4, 4),
  ]);
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

test('every size a widget declares fits on as few grids as hold them, each once, none over another', () => {
  const sizes = sizesUpTo({ columns: 4, rows: 4 }, { columns: 2, rows: 2 });
  const pages = sizePages(sizes);
  assert.equal(pages.length, 2);
  const placed = pages.flat();
  assert.deepEqual(
    placed.map(({ w, h }) => `${w}x${h}`).sort(),
    sizes.map(({ columns, rows }) => `${columns}x${rows}`).sort(),
  );
  for (const page of pages)
    for (const [index, a] of page.entries()) {
      assert(
        a.x >= 0 && a.y >= 0 && a.x + a.w <= 12 && a.y + a.h <= 7,
        `${a.w}x${a.h} leaves the grid`,
      );
      for (const b of page.slice(index + 1))
        assert(
          a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
          `${a.w}x${a.h} covers ${b.w}x${b.h}`,
        );
    }
  // The largest first, at the top left of the first grid.
  assert.deepEqual(pages[0]![0], { x: 0, y: 0, w: 4, h: 4 });
  // One the grid holds whole is on a grid of its own.
  assert.deepEqual(
    sizePages([
      { columns: 12, rows: 7 },
      { columns: 1, rows: 1 },
    ]).length,
    2,
  );
});
