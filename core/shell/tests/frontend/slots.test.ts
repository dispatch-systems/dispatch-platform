import test from 'node:test';
import assert from 'node:assert/strict';
import { Award, Shirt } from 'lucide-react';
import {
  collectionLabels,
  connectionCard,
  connectionCards,
  installFeatures,
  readToggles,
  switchIcon,
  type CollectionLabels,
  type ConnectionCard,
  type ReadToggles,
} from '../../frontend/runtime/slots.js';

// Synthetic owners, installed as the app installs its manifests.
const group = (label: string, order: number): ReadToggles => ({
  label,
  missing: `${label.toLowerCase()} data`,
  order,
  sources: {},
  toggles: [],
});

test('read toggles come group by group in their order, ties in the order the owners are listed', () => {
  installFeatures([
    { name: 'alpha', readToggles: group('Alpha', 20) },
    { name: 'beta' },
    { name: 'gamma', readToggles: group('Gamma', 10) },
    { name: 'delta', readToggles: group('Delta', 20) },
  ]);
  assert.deepEqual(
    readToggles().map((each) => each.label),
    ['Gamma', 'Alpha', 'Delta'],
  );
});

test("a page's switch shows the icon its feature declares", () => {
  installFeatures([
    { name: 'alpha', switch: { id: 'uniforms', icon: Shirt } },
    { name: 'beta' },
    { name: 'gamma', switch: { id: 'scorecard', icon: Award } },
  ]);
  assert.equal(switchIcon('uniforms'), Shirt);
  assert.equal(switchIcon('scorecard'), Award);
  assert.equal(switchIcon('timecard'), undefined);
});

test('connection cards come in the order their collectors are listed', () => {
  const card = (provider: ConnectionCard['provider']): ConnectionCard => ({
    provider,
    read: `/api/dsp/connections/${provider}`,
    load: () => Promise.resolve(),
    render: () => provider,
  });
  installFeatures([
    { name: 'gamma', connectionCard: card('cortex') },
    { name: 'beta' },
    { name: 'alpha', connectionCard: card('paycom') },
  ]);
  assert.deepEqual(
    connectionCards().map((each) => each.provider),
    ['cortex', 'paycom'],
  );
  assert.equal(connectionCard('paycom')?.read, '/api/dsp/connections/paycom');
  assert.equal(connectionCard('other'), undefined);
});

test('collections come with the collector that runs them, in the order they are declared', () => {
  const collection = (kind: string): CollectionLabels => ({
    kind,
    schedule: { id: kind, label: kind },
    unit: 'item',
    count: (metrics) => metrics.rows,
  });
  installFeatures([
    { name: 'beta', collections: [collection('beta.b'), collection('beta.a')] },
    { name: 'gamma' },
    { name: 'alpha', collections: [collection('alpha.a')] },
  ]);
  assert.deepEqual(
    collectionLabels().map(({ provider, kind }) => [provider, kind]),
    [
      ['beta', 'beta.b'],
      ['beta', 'beta.a'],
      ['alpha', 'alpha.a'],
    ],
  );
});
