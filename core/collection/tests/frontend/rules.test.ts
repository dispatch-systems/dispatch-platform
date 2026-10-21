import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { ScheduleRuleInput } from '../../api/index.js';
import { ruleText } from '../../frontend/collectors/rules.js';

// The page words a rule as the server does in the activity log: the same table holds both
// (`tests/backend/rules.rs`).
const table = new URL('../support/rule-words.json', import.meta.url);
const words: [Partial<ScheduleRuleInput>, string][] = JSON.parse(readFileSync(table, 'utf8'));

test('a rule reads on the page as the activity log words it', () => {
  for (const [rule, expected] of words) {
    const input = {
      id: null,
      collects: 'paycom',
      times: [],
      every: null,
      from: null,
      to: null,
      ...rule,
    } as ScheduleRuleInput;
    assert.equal(ruleText(input), expected);
  }
});
