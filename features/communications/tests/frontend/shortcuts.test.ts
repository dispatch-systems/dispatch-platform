import assert from 'node:assert/strict';
import test from 'node:test';
import { shortcut } from '../../frontend/shared/shortcuts.js';

/** `key` pressed with nothing held, as `held` changes it. */
const press = (key: string, held: Partial<KeyboardEvent> = {}) => ({
  key,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  isComposing: false,
  defaultPrevented: false,
  ...held,
});

test('the page’s keys move about it, but while a field is written in, where Escape alone leaves it, or with a key held or taken', () => {
  const pressed = ['j', 'k', 'n', 'e', '/', 'Escape', 'J', 'x'].map((key) =>
    shortcut(press(key), false),
  );
  assert.deepEqual(pressed, [
    'next',
    'previous',
    'waiting',
    'done',
    'search',
    undefined,
    undefined,
    undefined,
  ]);
  // Written in a field, each key is the field's, but Escape.
  assert.deepEqual(
    ['j', 'n', 'e', '/', 'Escape'].map((key) => shortcut(press(key), true)),
    [undefined, undefined, undefined, undefined, 'leave'],
  );
  // With Ctrl, Alt or ⌘ held, mid-composition, or taken by something else, none is the page's.
  for (const held of [
    { ctrlKey: true },
    { metaKey: true },
    { altKey: true },
    { isComposing: true },
    { defaultPrevented: true },
  ]) {
    assert.equal(shortcut(press('j', held), false), undefined);
    assert.equal(shortcut(press('Escape', held), true), undefined);
  }
});
