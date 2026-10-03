import test from 'node:test';
import assert from 'node:assert/strict';
import '../support/manifests.js';
import { readToggles } from '../../../core/shell/frontend/runtime/slots.js';

const once = (ids: readonly string[], what: string) =>
  assert.deepEqual(
    ids.filter((id, index) => ids.indexOf(id) !== index),
    [],
    `${what} declared more than once`,
  );

test('each kind of data agents may read is declared once, and each group has its own place', () => {
  once(
    readToggles().flatMap((group) => group.toggles.map((toggle) => toggle.id)),
    'read toggles',
  );
  once(
    readToggles().map((group) => String(group.order)),
    'read toggle group orders',
  );
});
