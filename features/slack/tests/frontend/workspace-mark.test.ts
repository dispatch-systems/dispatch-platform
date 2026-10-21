import assert from 'node:assert/strict';
import test from 'node:test';
import { initials } from '../../frontend/workspace-mark.js';

test('a workspace without a logo is marked by the first letters of its first two words, or of its one word', () => {
  assert.deepEqual(['BHL Drivers', 'Drivers', 'blue heron logistics', ''].map(initials), [
    'BD',
    'DR',
    'BH',
    '?',
  ]);
});
