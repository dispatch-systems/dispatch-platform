import test from 'node:test';
import assert from 'node:assert/strict';
import '../support/manifests.js';
import { featureCatalog } from '../../../core/shell/frontend/runtime/features.js';
import { features } from '../../frontend/features.js';
import {
  connectionCard,
  connectionCards,
  readToggles,
  switchIcon,
} from '../../../core/shell/frontend/runtime/slots.js';

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

test("each page's switch has its icon, declared once", () => {
  once(
    features.flatMap((feature) => (feature.switch ? [feature.switch.id] : [])),
    'switches',
  );
  for (const page of featureCatalog.filter((entry) => entry.kind === 'page'))
    assert(switchIcon(page.id), `${page.id} has no icon`);
});

test('each connection has one card', () => {
  once(
    connectionCards().map((card) => card.provider),
    'connection cards',
  );
  for (const connection of featureCatalog.filter((entry) => entry.kind === 'connection'))
    assert(connectionCard(connection.id), `${connection.id} has no card`);
});
