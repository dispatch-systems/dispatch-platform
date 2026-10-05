import assert from 'node:assert/strict';

/**
 * Fails unless a check found no violation, naming each one it found. Every rule holds
 * absolutely: there is no list of tolerated violations.
 */
export function holds(rule: string, check: string, violations: Iterable<string>) {
  const found = [...new Set(violations)].sort();
  assert.deepEqual(
    found,
    [],
    `${rule}: ${check}. Fix each violation:\n${found.map((violation) => `  ${violation}`).join('\n')}`,
  );
}
