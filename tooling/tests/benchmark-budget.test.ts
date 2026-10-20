import test from 'node:test';
import assert from 'node:assert/strict';
import {
  checkBenchmark,
  checkBeside,
  budgets,
  tolerances,
  type BenchmarkReport,
} from '../benchmarks/benchmark-budget.js';
import { DatabaseWork } from '../benchmarks/database-work.js';
const sample = (): BenchmarkReport => ({
  employeesPerDsp: 3000,
  startupMs: 50,
  idleRssBytes: 12 * 1024 ** 2,
  peakRssBytes: 80 * 1024 ** 2,
  afterLoadRssBytes: 20 * 1024 ** 2,
  completedCollections: 4,
  observedRunning: 8,
  tableReads: (['daily', 'meals', 'gaps'] as const).flatMap((route) =>
    (['cold', 'warm'] as const).map((temperature) => ({
      route,
      temperature,
      status: 200,
      ms: 5,
      bytes: 10000,
      ...(route === 'gaps' ? {} : { rows: 100, total: 3000 }),
    })),
  ),
  coldBurst: {
    heavyStatuses: Array(8).fill(200),
    light: Array.from({ length: 8 }, () => ({ status: 200, ms: 5 })),
  },
  databaseWork: {
    attributed: 1,
    mismatched: 0,
    routes: [{ route: '/api/session', calls: 1, queueMs: 1, lockMs: 0, workMs: 30 }],
  },
  measurements: (['single-dsp', 'multi-dsp-collection'] as const).flatMap((scenario) =>
    [1, 4, 8, 16].map((concurrency) => ({
      scenario,
      concurrency,
      requests: 240,
      errors: 0,
      requestsPerSecond: 100,
      medianMs: 20,
      p95Ms: 50,
      responseBytes: 100000,
    })),
  ),
});
test('benchmark failures cannot pass as successful measurements', () => {
  checkBenchmark(sample(), true);
  for (const corrupt of [
    (r: BenchmarkReport) => {
      r.tableReads.pop();
    },
    (r: BenchmarkReport) => {
      r.tableReads[0]!.rows = 101;
    },
    (r: BenchmarkReport) => {
      r.tableReads[0]!.total = 100;
    },
    (r: BenchmarkReport) => {
      r.tableReads[0]!.bytes = budgets.dailyPageBytes + 1;
    },
    (r: BenchmarkReport) => {
      r.tableReads[2]!.bytes = budgets.mealPageBytes + 1;
    },
    (r: BenchmarkReport) => {
      r.coldBurst.light[0]!.status = 503;
    },
    (r: BenchmarkReport) => {
      r.coldBurst.heavyStatuses.pop();
    },
    (r: BenchmarkReport) => {
      r.databaseWork.attributed = 0;
    },
    (r: BenchmarkReport) => {
      r.databaseWork.mismatched = 1;
    },
    (r: BenchmarkReport) => {
      r.measurements[0]!.errors = 1;
    },
    (r: BenchmarkReport) => {
      r.measurements.pop();
    },
    (r: BenchmarkReport) => {
      r.measurements[0]!.requests = 1;
    },
    (r: BenchmarkReport) => {
      r.measurements[0]!.p95Ms = NaN;
    },
    (r: BenchmarkReport) => {
      r.completedCollections = 0;
    },
    (r: BenchmarkReport) => {
      r.observedRunning = 0;
    },
  ]) {
    const report = sample();
    corrupt(report);
    assert.throws(() => checkBenchmark(report));
  }
  for (const corrupt of [
    (r: BenchmarkReport) => {
      r.coldBurst.light[0]!.ms = budgets.p95Ms + 1;
    },
    (r: BenchmarkReport) => {
      r.tableReads[0]!.ms = budgets.p95Ms + 1;
    },
    (r: BenchmarkReport) => {
      r.measurements[0]!.p95Ms = budgets.p95Ms + 1;
    },
    (r: BenchmarkReport) => {
      r.measurements[0]!.requestsPerSecond = budgets.requestsPerSecond - 1;
    },
    (r: BenchmarkReport) => {
      r.peakRssBytes = budgets.peakRssBytes + 1;
    },
  ]) {
    const report = sample();
    corrupt(report);
    checkBenchmark(report);
    assert.throws(() => checkBenchmark(report, true));
  }
});
test('beside its base on the same runner, a build is judged by both, and the runner by the base', () => {
  const judge = (change: (base: BenchmarkReport, build: BenchmarkReport) => void) => {
    const [base, build] = [sample(), sample()];
    change(base, build);
    return () => checkBenchmark(build, true, base);
  };
  // A runner whose base breaks a budget can't hold it: the base alone judges the build there.
  const heavy = budgets.peakRssBytes + 40 * 1024 ** 2;
  assert.doesNotThrow(
    judge((base, build) => {
      base.peakRssBytes = heavy;
      build.peakRssBytes = heavy + 4 * 1024 ** 2;
    }),
  );
  assert.doesNotThrow(
    judge((base, build) => {
      base.measurements[0]!.p95Ms = budgets.p95Ms + 100;
      build.measurements[0]!.p95Ms = budgets.p95Ms + 150;
    }),
  );
  // A base within the budget keeps it binding.
  assert.throws(
    judge((base, build) => {
      base.peakRssBytes = budgets.peakRssBytes - 1024 ** 2;
      build.peakRssBytes = budgets.peakRssBytes + 1;
    }),
    /peakRssBytes exceeds budget/,
  );
  // A build clearly worse than its base fails, though within every budget.
  for (const worse of [
    (base: BenchmarkReport, build: BenchmarkReport) => {
      build.peakRssBytes = Math.ceil(base.peakRssBytes * tolerances.memory) + 1;
    },
    (base: BenchmarkReport, build: BenchmarkReport) => {
      build.idleRssBytes = Math.ceil(base.idleRssBytes * tolerances.memory) + 1;
    },
    (base: BenchmarkReport, build: BenchmarkReport) => {
      build.startupMs = base.startupMs * tolerances.startup + tolerances.startupMs + 1;
    },
    (base: BenchmarkReport, build: BenchmarkReport) => {
      build.measurements[3]!.p95Ms = base.measurements[3]!.p95Ms + tolerances.p95Ms + 1;
    },
    (base: BenchmarkReport, build: BenchmarkReport) => {
      build.measurements[5]!.requestsPerSecond =
        base.measurements[5]!.requestsPerSecond * tolerances.throughput - 1;
    },
  ])
    assert.throws(judge(worse), /the base's/);
  // Noise within the tolerances passes.
  assert.doesNotThrow(
    judge((base, build) => {
      build.peakRssBytes = Math.floor(base.peakRssBytes * 1.1);
      build.measurements[3]!.p95Ms = base.measurements[3]!.p95Ms + 30;
      build.measurements[5]!.requestsPerSecond = base.measurements[5]!.requestsPerSecond * 0.8;
    }),
  );
});
test('measured again beside its base, a build is judged by the second run, and both must be correct', () => {
  const base = sample();
  base.peakRssBytes = budgets.peakRssBytes - 1024 ** 2;
  const missed = sample();
  missed.peakRssBytes = budgets.peakRssBytes + 1;
  assert.doesNotThrow(() => checkBeside([missed, sample()], base));
  assert.throws(() => checkBeside([missed, missed], base), /peakRssBytes exceeds budget/);
  const failed = sample();
  failed.measurements[0]!.errors = 1;
  assert.throws(() => checkBeside([failed, sample()], base), /HTTP errors/);
});
test('an HTTP error names the failed route, status and response', () => {
  const report = sample();
  report.measurements[0]!.errors = 1;
  report.failures = [
    {
      scenario: report.measurements[0]!.scenario,
      concurrency: report.measurements[0]!.concurrency,
      route: '/api/session',
      tenant: 0,
      status: 503,
      body: '{"error":"busy"}',
    },
  ];
  assert.throws(() => checkBenchmark(report), /\/api\/session returned 503: \{"error":"busy"\}/);
});

test('workload diagnostics join split log frames by request identity and retain separate DSPs', () => {
  const log = new DatabaseWork();
  const events = [
    { event: 'database.slow', fields: { requestId: null, route: null, workMs: 100 } },
    {
      event: 'database.slow',
      fields: { requestId: 'r1', route: '/a', dspId: 'dsp1', queueMs: 20, lockMs: 3, workMs: 60 },
    },
    {
      event: 'database.slow',
      fields: { requestId: 'r2', route: '/b', dspId: 'dsp2', queueMs: 0, lockMs: 0, workMs: 26 },
    },
    {
      event: 'database.slow',
      fields: { requestId: 'r1', route: '/a', dspId: 'dsp1', queueMs: 0, lockMs: 0, workMs: 30 },
    },
    { event: 'http.request', fields: { requestId: 'r2', route: '/b', dspId: 'dsp2' } },
    { event: 'http.request', fields: { requestId: 'r1', route: '/a', dspId: 'dsp1' } },
  ]
    .map((event) => JSON.stringify(event) + '\n')
    .join('');
  log.feed(events.slice(0, 41));
  log.feed(events.slice(41, 204));
  log.feed(events.slice(204));
  const expected = {
    attributed: 3,
    mismatched: 0,
    routes: [
      { route: '/a', calls: 2, queueMs: 20, lockMs: 3, workMs: 60 },
      { route: '/b', calls: 1, queueMs: 0, lockMs: 0, workMs: 26 },
    ],
  };
  assert.deepEqual(log.report(), expected);
  assert.deepEqual(log.report(), expected, 'Reading diagnostics never counts an event twice');
  log.feed(
    JSON.stringify({
      event: 'database.slow',
      fields: { requestId: 'r3', route: '/a', dspId: 'dsp2', queueMs: 0, lockMs: 0, workMs: 30 },
    }) + '\n',
  );
  log.feed(
    JSON.stringify({
      event: 'http.request',
      fields: { requestId: 'r3', route: '/a', dspId: 'dsp3' },
    }) + '\n',
  );
  assert.equal(log.report().mismatched, 1);
});
