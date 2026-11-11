import test from 'node:test';
import assert from 'node:assert/strict';
import { featureAt } from '../shared/features.js';

test('custom file ownership retains both the DSP and folder across every source kind', () => {
  for (const leaf of [
    'backend/store.rs',
    'frontend/feature.ts',
    'tests/api/log.test.ts',
    'contract/Cargo.toml',
  ]) {
    assert.deepEqual(featureAt(`features/custom/fscl/fuel_log/${leaf}`), {
      dir: 'features/custom/fscl/fuel_log',
      name: 'fscl_fuel_log',
      owner: 'fscl',
    });
    assert.equal(featureAt(`features/custom/other/fuel_log/${leaf}`)?.name, 'other_fuel_log');
  }
  for (const file of [
    'features/custom',
    'features/custom/fscl',
    'features/custom/BAD/fuel_log/x.rs',
  ])
    assert.equal(featureAt(file), undefined);
});
