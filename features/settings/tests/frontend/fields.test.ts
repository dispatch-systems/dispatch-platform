import assert from 'node:assert/strict';
import test from 'node:test';
import { checking, listed } from '../../frontend/fields/options.js';

// A field's options are what its feature lists as Settings is read, which can drop a value saved
// before, as a department no longer on the roster: the field keeps it, by its value, until the
// member unchecks it, rather than an edit to another option losing it.
test('a value saved before its list dropped it stays checked through an edit until unchecked', () => {
  const options = [
    { value: 'DSP', label: 'DSP' },
    { value: 'Helpers', label: 'Helpers' },
  ];
  const saved = ['DSP', 'Retired'];
  const shown = listed(options, saved);
  assert.deepEqual(shown, [...options, { value: 'Retired', label: 'Retired' }]);
  assert.deepEqual(checking(shown, saved, 'Helpers', true), ['DSP', 'Helpers', 'Retired']);
  const unchecked = checking(shown, saved, 'Retired', false);
  assert.deepEqual(unchecked, ['DSP']);
  // Unchecked, it's still there to check again until the edit is saved.
  assert.deepEqual(listed(options, [...saved, ...unchecked]), shown);
});
