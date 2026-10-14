import assert from 'node:assert/strict';

export interface Measurement {
  scenario: 'single-dsp' | 'multi-dsp-collection';
  concurrency: number;
  requests: number;
  errors: number;
  requestsPerSecond: number;
  medianMs: number;
  p95Ms: number;
  responseBytes: number;
}
export interface Failure {
  scenario: Measurement['scenario'];
  concurrency: number;
  route: string;
  tenant: number;
  status: number;
  body: string;
}
export interface BenchmarkReport {
  employeesPerDsp: number;
  startupMs: number;
  idleRssBytes: number;
  peakRssBytes: number;
  afterLoadRssBytes: number;
  completedCollections: number;
  observedRunning: number;
  measurements: Measurement[];
  failures?: Failure[];
  tableReads: TableRead[];
  coldBurst: {
    heavyStatuses: number[];
    light: { status: number; ms: number }[];
  };
  databaseWork: {
    attributed: number;
    mismatched: number;
    routes: { route: string; calls: number; queueMs: number; lockMs: number; workMs: number }[];
  };
}
export interface TableRead {
  route: 'daily' | 'meals' | 'gaps';
  temperature: 'cold' | 'warm';
  status: number;
  ms: number;
  bytes: number;
  rows?: number;
  total?: number;
}
// Deliberately broad across host/CI hardware; compare runs through the measurements
// CI preserves. Never auto-adjust these budgets to accept a failed run.
export const budgets = {
  startupMs: 5000,
  idleRssBytes: 64 * 1024 ** 2,
  peakRssBytes: 256 * 1024 ** 2,
  p95Ms: 2000,
  requestsPerSecond: 10,
  dailyPageBytes: 200 * 1024,
  mealPageBytes: 500 * 1024,
};
export function checkBenchmark(report: BenchmarkReport, performance = false) {
  assert.equal(report.tableReads.length, 6, 'Incomplete cold/warm read coverage');
  for (const route of ['daily', 'meals', 'gaps'] as const)
    for (const temperature of ['cold', 'warm'] as const) {
      const selected = report.tableReads.filter(
        (row) => row.route === route && row.temperature === temperature,
      );
      assert.equal(selected.length, 1, `Missing/duplicate table read: ${route}/${temperature}`);
      const row = selected[0]!;
      assert.equal(row.status, 200, `${route}/${temperature}: HTTP error`);
      assert(Number.isFinite(row.ms) && row.ms > 0 && row.bytes > 0, 'Invalid table measurement');
      if (route !== 'gaps') {
        assert.equal(row.rows, 100, 'Table pages must remain bounded');
        assert.equal(row.total, report.employeesPerDsp, 'Table totals must cover the whole DSP');
        assert(
          row.bytes <= (route === 'daily' ? budgets.dailyPageBytes : budgets.mealPageBytes),
          `${route}: decoded response exceeds page budget`,
        );
      }
      if (performance)
        assert(row.ms <= budgets.p95Ms, `${route}/${temperature}: read exceeds latency budget`);
    }
  assert.equal(report.coldBurst.heavyStatuses.length, 8, 'Incomplete cold burst');
  assert.equal(report.coldBurst.light.length, 8, 'Incomplete lightweight progress coverage');
  for (const status of report.coldBurst.heavyStatuses)
    assert.equal(status, 200, 'Cold read failed');
  for (const sample of report.coldBurst.light) {
    assert.equal(sample.status, 200, 'A cold burst starved a session read');
    assert(Number.isFinite(sample.ms) && sample.ms > 0, 'Invalid session measurement');
    if (performance) assert(sample.ms <= budgets.p95Ms, 'Session progress exceeds latency budget');
  }
  assert(report.databaseWork.attributed > 0, 'No slow HTTP database work was attributed');
  assert.equal(
    report.databaseWork.mismatched,
    0,
    'Slow database work must match its completed request',
  );
  for (const row of report.databaseWork.routes) {
    assert(typeof row.route === 'string' && row.route.startsWith('/'), 'Missing safe route label');
    assert(row.calls > 0);
    for (const key of ['queueMs', 'lockMs', 'workMs'] as const)
      assert(Number.isFinite(row[key]) && row[key] >= 0, 'Invalid database work measurement');
  }
  assert.equal(report.measurements.length, 8, 'Incomplete benchmark coverage');
  for (const scenario of ['single-dsp', 'multi-dsp-collection'])
    for (const concurrency of [1, 4, 8, 16]) {
      const rows = report.measurements.filter(
        (row) => row.scenario === scenario && row.concurrency === concurrency,
      );
      assert.equal(rows.length, 1, `Missing/duplicate workload: ${scenario}/${concurrency}`);
      const row = rows[0]!;
      assert.equal(row.requests, 240, 'Incomplete request workload');
      const failed = (report.failures ?? [])
        .filter((f) => f.scenario === scenario && f.concurrency === concurrency)
        .map((f) => `${f.route} returned ${f.status}: ${f.body}`);
      assert.equal(
        row.errors,
        0,
        [`${scenario}/${concurrency}: HTTP errors`, ...failed].join('\n'),
      );
      for (const value of [row.requestsPerSecond, row.p95Ms, row.medianMs, row.responseBytes])
        assert(Number.isFinite(value) && value > 0, 'Invalid workload measurement');
      if (performance) {
        assert(
          row.p95Ms <= budgets.p95Ms,
          `${scenario}/${concurrency}: p95 ${row.p95Ms} exceeds ${budgets.p95Ms}ms`,
        );
        assert(
          row.requestsPerSecond >= budgets.requestsPerSecond,
          `${scenario}/${concurrency}: throughput below budget`,
        );
      }
    }
  assert(report.completedCollections > 0, 'No collections completed during the read workload');
  assert(report.observedRunning > 0, 'No running collection overlapped the read workload');
  for (const key of ['startupMs', 'idleRssBytes', 'peakRssBytes', 'afterLoadRssBytes'] as const)
    assert(Number.isFinite(report[key]) && report[key] > 0, `Invalid ${key}`);
  if (performance)
    for (const key of ['startupMs', 'idleRssBytes', 'peakRssBytes'] as const)
      assert(report[key] <= budgets[key], `${key} exceeds budget`);
}
