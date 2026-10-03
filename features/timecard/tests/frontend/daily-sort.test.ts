import test from 'node:test';
import assert from 'node:assert/strict';
import { sortDailyRows } from '../../frontend/daily-sort.js';
import type { DailyTimecard } from '../../../../shared/contracts/index.js';

test('daily sorting is numeric and keeps ascending codes for descending ties without mutating rows', () => {
  const row = (employeeCode: string, hours: number): DailyTimecard => ({
    employeeCode,
    hours,
    name: 'Same',
    date: '2026-09-22',
    status: 'Complete',
    punches: [],
  });
  const rows = [row('B', 8), row('A', 8), row('C', 12)];
  assert.deepEqual(
    sortDailyRows(rows, 'totalHours', true).map((r) => r.employeeCode),
    ['C', 'A', 'B'],
  );
  assert.deepEqual(
    sortDailyRows(rows, 'name', true).map((r) => r.employeeCode),
    ['A', 'B', 'C'],
  );
  assert.deepEqual(
    rows.map((r) => r.employeeCode),
    ['B', 'A', 'C'],
  );
});
