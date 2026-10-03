import test from 'node:test';
import assert from 'node:assert/strict';
import { Award, Shirt } from 'lucide-react';
import {
  connectionCard,
  connectionCards,
  installFeatures,
  readToggles,
  switchIcon,
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
