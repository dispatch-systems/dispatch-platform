import test from 'node:test';
import assert from 'node:assert/strict';
import type { ConductOverview, ConductWriteupRow } from '../../api/index.js';
import { emptyDraft, filled, filledOf, refill } from '../../frontend/compose/draft.js';

const writeup = (
  id: string,
  tier: ConductWriteupRow['tier'],
  writtenOn: string,
): ConductWriteupRow => ({
  id,
  person: 'ana',
  personName: 'Ana Diaz',
  categoryId: 'tardies',
  tier,
  finalKind: null,
  suspension: null,
  writtenOn,
  supervisorName: 'Casey Supervisor',
  signedOn: writtenOn,
  hasCopy: false,
  revision: 2,
});

const category = (id: string, name: string, startTier: ConductWriteupRow['tier']) => ({
  id,
  name,
  startTier,
  statement: `${name} statement on [date].`,
  prior: `${name} prior.`,
  plan: `${name} plan.`,
  revision: 1,
  retired: false,
});

const overview: ConductOverview = {
  today: '2026-11-08',
  countsFrom: '2025-11-08',
  categories: [
    category('tardies', 'Excessive Tardies', 'verbal'),
    category('damage', 'Vehicle Damage', 'final'),
  ],
  people: [{ code: 'ana', name: 'Ana Diaz', photo: null, left: false }],
  // The newest first; the oldest no longer counts.
  writeups: [
    writeup('w2', 'written', '2026-08-23'),
    writeup('w1', 'verbal', '2026-08-09'),
    writeup('w0', 'final', '2025-06-01'),
  ],
  standings: [],
};

test('choosing someone and a category suggests the next tier and names the writeups that still count', () => {
  assert.deepEqual(filled(overview, 'ana', 'tardies'), {
    tier: 'final',
    statement: 'Excessive Tardies statement on [date].',
    prior:
      'Excessive Tardies prior.\nPrevious writeups for Excessive Tardies: Verbal Warning 8/9/2026, Written Warning 8/23/2026.',
    plan: 'Excessive Tardies plan.',
  });
  // Editing a writeup leaves it out of its own history.
  assert.equal(filled(overview, 'ana', 'tardies', 'w2')?.tier, 'written');
  // A category that starts higher starts there.
  assert.equal(filled(overview, 'ana', 'damage')?.tier, 'final');
});

test('choosing again refills only what nobody changed since it was filled', () => {
  const start = emptyDraft(overview.today, 'ana');
  const first = refill(
    { ...start, category: 'tardies' },
    filledOf(start),
    filled(overview, 'ana', 'tardies'),
  );
  assert.equal(first.draft.tier, 'final');
  const typed = { ...first.draft, statement: 'DA was late on 10/30.', tier: 'written' as const };
  const again = refill(
    { ...typed, category: 'damage' },
    first.before,
    filled(overview, 'ana', 'damage'),
  );
  assert.equal(again.draft.statement, 'DA was late on 10/30.');
  assert.equal(again.draft.tier, 'written');
  assert.equal(again.draft.plan, 'Vehicle Damage plan.');
  assert.equal(again.draft.prior, 'Vehicle Damage prior.');
});
