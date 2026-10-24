import assert from 'node:assert/strict';
import test from 'node:test';
import { frame } from '../../frontend/display/frame.js';

// Message display's preview of a desktop, in the room beside the choices, a 1440 by 900 window.
const window = { width: 1440, height: 900 };
const desktop = (width: number, list = 340) =>
  frame({ room: { width, height: 700 }, phone: false, fit: false, list, window });

test('a desktop shows at its actual size: the whole page with room for both panes, else the conversation alone while its page has two', () => {
  assert.deepEqual(desktop(1000), { width: 1000, height: 700, scale: 1, aside: 0 });
  // The page laid out wider by its list, which is kept out of view.
  assert.deepEqual(desktop(600), { width: 940, height: 700, scale: 1, aside: 340 });
  assert.deepEqual(desktop(500, 400), { width: 900, height: 700, scale: 1, aside: 400 });
  // Too narrow for two panes even so: the page as wide as the room, one pane, nothing kept out.
  assert.deepEqual(desktop(450), { width: 450, height: 700, scale: 1, aside: 0 });
});

test('to fit, or with no room for its actual size, a desktop’s whole page is made smaller', () => {
  const fit = frame({
    room: { width: 600, height: 700 },
    phone: false,
    fit: true,
    list: 340,
    window,
  });
  assert.deepEqual([fit.width, fit.height, fit.aside], [1200, 750, 0]);
  assert.equal(fit.scale, 0.5);
  assert.equal(desktop(400).scale < 1, true);
});

test('a phone shows at its actual size, inside its edge', () => {
  const phone = (width: number, height: number) =>
    frame({ room: { width, height }, phone: true, fit: false, list: 340, window });
  assert.deepEqual(phone(660, 900), { width: 390, height: 760, scale: 1, aside: 0 });
  assert.deepEqual(phone(354, 440), { width: 334, height: 420, scale: 1, aside: 0 });
});
