import test from 'node:test';
import assert from 'node:assert/strict';
import type { ConductOverview, ConductWriteupRow } from '../../api/index.js';
import { emptyDraft, filled, filledOf, refill } from '../../frontend/compose/draft.js';
import { record } from '../../frontend/tiers.js';

const writeup = (
  id: string,
  tier: ConductWriteupRow['tier'],
  writtenOn: string,
  categoryId = 'tardies',
): ConductWriteupRow => ({
  id,
  person: 'ana',
  personName: 'Ana Diaz',
  categoryId,
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
  stepDown: null,
  statement: `${name} statement on [date].`,
  prior: `${name} prior.`,
  plan: `${name} plan.`,
  revision: 1,
  retired: false,
});

const overview: ConductOverview = {
  today: '2026-11-08',
  categories: [
    category('tardies', 'Excessive Tardies', 'verbal'),
    category('damage', 'Vehicle Damage', 'final'),
  ],
  people: [{ code: 'ana', name: 'Ana Diaz', photo: null, left: false }],
  // The newest first. Ana's Final from last year had stepped down to a clean record by her
  // Verbal, and her Vehicle Damage writeup has since.
  writeups: [
    writeup('w2', 'written', '2026-08-23'),
    writeup('w1', 'verbal', '2026-08-09'),
    writeup('d0', 'final', '2025-09-01', 'damage'),
    writeup('w0', 'final', '2025-06-01'),
  ],
  standings: [
    {
      person: 'ana',
      categoryId: 'tardies',
      tier: 'written',
      latestId: 'w2',
      latestOn: '2026-08-23',
      stepsDownOn: '2027-02-23',
      stepsDownTo: 'verbal',
    },
  ],
  stoodBefore: { w2: 'verbal' },
};

test('choosing someone and a category suggests the tier above where they stand and names their writeups since their record was clean', () => {
  assert.deepEqual(filled(overview, 'ana', 'tardies'), {
    tier: 'final',
    statement: 'Excessive Tardies statement on [date].',
    prior:
      'Excessive Tardies prior.\nPrevious writeups for Excessive Tardies: Verbal Warning 8/9/2026, Written Warning 8/23/2026.',
    plan: 'Excessive Tardies plan.',
  });
  // Editing a writeup looks from where they stood just before it.
  assert.deepEqual(record(overview, 'ana', 'tardies', 'w2'), {
    stood: 'verbal',
    earlier: [overview.writeups[1]],
  });
  assert.equal(filled(overview, 'ana', 'tardies', 'w2')?.tier, 'written');
  // A clean record starts where the category does, naming nothing before.
  assert.deepEqual(filled(overview, 'ana', 'damage'), {
    tier: 'final',
    statement: 'Vehicle Damage statement on [date].',
    prior: 'Vehicle Damage prior.',
    plan: 'Vehicle Damage plan.',
  });
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
