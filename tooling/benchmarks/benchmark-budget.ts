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
  // Main's own builds peaked at 239-335 MB in October 2026's queue runs, and a build measured
  // first on a runner reads higher than the same build after it: this leaves them room, so
  // only growth fails it.
  peakRssBytes: 384 * 1024 ** 2,
  p95Ms: 2000,
  requestsPerSecond: 10,
  dailyPageBytes: 200 * 1024,
  mealPageBytes: 500 * 1024,
};
/**
 * How much worse than the base's build the candidate may measure, both run on the same runner
 * one after the other. Runs of one build differ widely: across twenty CI runs peak memory
 * spanned 193-286 MB and each workload's p95 two to three times. So only a clear regression
 * fails; the budgets still bind wherever the base fits them. Never auto-adjust these to
 * accept a failed run either.
 */
export const tolerances = {
  /** Idle and peak memory: up to this many times the base's. */
  memory: 1.4,
  /** Startup: up to this many times the base's, and this many milliseconds more. */
  startup: 3,
  startupMs: 200,
  /** Each workload's p95: up to this many times the base's, and this many milliseconds more. */
  p95: 3,
  p95Ms: 100,
  /** Each workload's throughput: at least this share of the base's. */
  throughput: 0.4,
};
/**
 * Whether the budget holds `value`. A base that broke the same budget on the same runner
 * shows the runner can't hold it, so then only the comparison with the base judges.
 */
function within(value: number, budget: number, base: number | undefined, at: 'most' | 'least') {
  const holds = (v: number) => (at === 'most' ? v <= budget : v >= budget);
  return holds(value) || (base !== undefined && !holds(base));
}
export function checkBenchmark(
  report: BenchmarkReport,
  performance = false,
  baseline?: BenchmarkReport,
) {
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
      const base = baseline?.tableReads.find(
        (other) => other.route === route && other.temperature === temperature,
      );
      if (performance)
        assert(
          within(row.ms, budgets.p95Ms, base?.ms, 'most'),
          `${route}/${temperature}: read exceeds latency budget`,
        );
    }
  assert.equal(report.coldBurst.heavyStatuses.length, 8, 'Incomplete cold burst');
  assert.equal(report.coldBurst.light.length, 8, 'Incomplete lightweight progress coverage');
  for (const status of report.coldBurst.heavyStatuses)
    assert.equal(status, 200, 'Cold read failed');
  const slowestBase = baseline && Math.max(...baseline.coldBurst.light.map(({ ms }) => ms));
  for (const sample of report.coldBurst.light) {
    assert.equal(sample.status, 200, 'A cold burst starved a session read');
    assert(Number.isFinite(sample.ms) && sample.ms > 0, 'Invalid session measurement');
    if (performance)
      assert(
        within(sample.ms, budgets.p95Ms, slowestBase, 'most'),
        'Session progress exceeds latency budget',
      );
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
      const base = baseline?.measurements.find(
        (other) => other.scenario === scenario && other.concurrency === concurrency,
      );
      if (performance) {
        assert(
          within(row.p95Ms, budgets.p95Ms, base?.p95Ms, 'most'),
          `${scenario}/${concurrency}: p95 ${row.p95Ms} exceeds ${budgets.p95Ms}ms`,
        );
        assert(
          within(
            row.requestsPerSecond,
            budgets.requestsPerSecond,
            base?.requestsPerSecond,
            'least',
          ),
          `${scenario}/${concurrency}: throughput below budget`,
        );
      }
      if (performance && base) {
        const p95 = Math.max(base.p95Ms * tolerances.p95, base.p95Ms + tolerances.p95Ms);
        assert(
          row.p95Ms <= p95,
          `${scenario}/${concurrency}: p95 ${row.p95Ms}ms against the base's ${base.p95Ms}ms`,
        );
        assert(
          row.requestsPerSecond >= base.requestsPerSecond * tolerances.throughput,
          `${scenario}/${concurrency}: ${row.requestsPerSecond} requests a second against the base's ${base.requestsPerSecond}`,
        );
      }
    }
  assert(report.completedCollections > 0, 'No collections completed during the read workload');
  assert(report.observedRunning > 0, 'No running collection overlapped the read workload');
  for (const key of ['startupMs', 'idleRssBytes', 'peakRssBytes', 'afterLoadRssBytes'] as const)
    assert(Number.isFinite(report[key]) && report[key] > 0, `Invalid ${key}`);
  if (performance)
    for (const key of ['startupMs', 'idleRssBytes', 'peakRssBytes'] as const)
      assert(within(report[key], budgets[key], baseline?.[key], 'most'), `${key} exceeds budget`);
  if (performance && baseline) {
    for (const key of ['idleRssBytes', 'peakRssBytes'] as const)
      assert(
        report[key] <= baseline[key] * tolerances.memory,
        `${key} ${report[key]} against the base's ${baseline[key]}`,
      );
    assert(
      report.startupMs <= baseline.startupMs * tolerances.startup + tolerances.startupMs,
      `startupMs ${report.startupMs} against the base's ${baseline.startupMs}`,
    );
  }
}
/** The key measurements of the candidate beside the base's, a line each. */
export function compared(report: BenchmarkReport, baseline: BenchmarkReport) {
  const mb = (bytes: number) => `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  const lines = [
    `startup: ${baseline.startupMs} ms, now ${report.startupMs} ms`,
    `idle memory: ${mb(baseline.idleRssBytes)}, now ${mb(report.idleRssBytes)}`,
    `peak memory: ${mb(baseline.peakRssBytes)}, now ${mb(report.peakRssBytes)}`,
  ];
  for (const row of report.measurements) {
    const base = baseline.measurements.find(
      (other) => other.scenario === row.scenario && other.concurrency === row.concurrency,
    );
    if (base)
      lines.push(
        `${row.scenario}/${row.concurrency}: p95 ${base.p95Ms} ms, now ${row.p95Ms} ms; ${base.requestsPerSecond}/s, now ${row.requestsPerSecond}/s`,
      );
  }
  return lines.map((line) => `  ${line}\n`).join('');
}
