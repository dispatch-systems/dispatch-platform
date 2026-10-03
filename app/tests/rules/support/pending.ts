import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

/**
 * `pending.json`: the violations the restructure's remaining steps remove, by rule and check,
 * each with the step that removes it. Temporary: it may only shrink, and is empty before the
 * restructure merges, when every rule holds absolutely.
 */
type Pending = Record<string, Record<string, Record<string, string>>>;
const file = path.join(import.meta.dirname, '..', 'pending.json');
const pending = (JSON.parse(fs.readFileSync(file, 'utf8')) as { pending: Pending }).pending;
const steps = new Set(['A4c', 'A5', 'A6', 'Phase B']);

/**
 * Fails on any violation `pending.json` does not list for this check, and on any it lists that
 * no longer occurs, so that the list is emptied as the steps land.
 */
export function holds(rule: string, check: string, violations: Iterable<string>) {
  const found = [...new Set(violations)].sort();
  const allowed = pending[rule]?.[check] ?? {};
  assert.deepEqual(
    {
      violations: found.filter((violation) => !(violation in allowed)),
      'no longer occur': Object.keys(allowed).filter((violation) => !found.includes(violation)),
    },
    { violations: [], 'no longer occur': [] },
    `${rule}: ${check}. Fix each violation; remove from app/tests/rules/pending.json each entry that no longer occurs`,
  );
}

/** Each rule's entries name its own checks and one of the restructure's steps. */
export function pendingNames(rule: string, checks: string[]) {
  for (const [check, entries] of Object.entries(pending[rule] ?? {})) {
    assert(checks.includes(check), `pending.json names ${rule}: ${check}, which is no check`);
    for (const [violation, step] of Object.entries(entries))
      assert(steps.has(step), `pending.json: ${violation} names ${step}, which is no step`);
  }
}
