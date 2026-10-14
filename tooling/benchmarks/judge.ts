import fs from 'node:fs';
import { parseArgs } from 'node:util';
import { checkBenchmark, compared, type BenchmarkReport } from './benchmark-budget.js';

// A workload report judged again beside the base's build, measured after it on the same runner:
// the merge queue's second look at a build that broke a budget there.
const { values } = parseArgs({
  options: { report: { type: 'string' }, baseline: { type: 'string' } },
});
const read = (file: string | undefined) =>
  JSON.parse(fs.readFileSync(file!, 'utf8')) as BenchmarkReport;
const report = read(values.report);
const baseline = read(values.baseline);
process.stdout.write(`Against the base's build:\n${compared(report, baseline)}`);
checkBenchmark(report, true, baseline);
