import test from 'node:test';
import assert from 'node:assert/strict';
import '../support/manifests.js';
import { featureCatalog } from '../../../core/shell/frontend/runtime/features.js';
import { features } from '../../frontend/features.js';
import { jobSchema } from '../../../shared/contracts/runtime.js';
import { errorLabel } from '../../../core/shell/frontend/runtime/api.js';
import {
  cacheRules,
  collectionLabels,
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

test('each job kind and schedule collection is named once, by a connection', () => {
  const collections = collectionLabels();
  once(
    collections.map((collection) => collection.kind),
    'job kinds',
  );
  once(
    collections.map((collection) => collection.schedule.id),
    'schedule collections',
  );
  for (const kind of jobSchema.shape.kind.options)
    assert(
      collections.some((collection) => collection.kind === kind),
      `${kind} has no collection`,
    );
  for (const { provider } of collections)
    assert(
      featureCatalog.some((entry) => entry.kind === 'connection' && entry.id === provider),
      `${provider} is no connection`,
    );
});

test('a read of collected data belongs to one owner', () => {
  const owners = cacheRules().map((rules) => rules.collected ?? []);
  owners.forEach((prefixes, owner) =>
    owners.forEach((others, other) => {
      if (other !== owner)
        for (const prefix of prefixes)
          for (const otherPrefix of others)
            assert(
              !prefix.startsWith(otherPrefix) && !otherPrefix.startsWith(prefix),
              `${prefix} and ${otherPrefix} overlap`,
            );
    }),
  );
});

test("each error code is worded once, and core's own wording doesn't hide an owner's", () => {
  const worded = features.flatMap((feature) =>
    Object.entries({ ...feature.errors, ...feature.scheduleIssues }),
  );
  once(
    worded.map(([code]) => code),
    'error codes',
  );
  for (const [code, label] of worded) assert.equal(errorLabel(code), label, code);
});
