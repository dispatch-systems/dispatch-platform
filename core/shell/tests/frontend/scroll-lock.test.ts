import test from 'node:test';
import assert from 'node:assert/strict';

const body = { style: { overflow: '' } };
Object.assign(globalThis, { document: { body } });
const { lockScroll } = await import('../../frontend/ui/scrollLock.js');

test('the page scrolls again once every dialog closes, whichever closes first', () => {
  const below = lockScroll();
  const above = lockScroll();
  below();
  assert.equal(body.style.overflow, 'hidden');
  above();
  assert.equal(body.style.overflow, '');
});
