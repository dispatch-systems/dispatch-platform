import test from 'node:test';
import assert from 'node:assert/strict';
import { installFeatures, readToggles, type ReadToggles } from '../../frontend/runtime/slots.js';

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
