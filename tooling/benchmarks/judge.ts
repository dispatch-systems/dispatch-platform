import fs from 'node:fs';
import { parseArgs } from 'node:util';
import { checkBeside, compared, type BenchmarkReport } from './benchmark-budget.js';

// A build's workload reports judged again beside the base's build, measured on the same runner:
// the second look at a build that broke a budget alone, and with a second report, the third.
const { values } = parseArgs({
  options: { report: { type: 'string', multiple: true }, baseline: { type: 'string' } },
});
const read = (file: string) => JSON.parse(fs.readFileSync(file, 'utf8')) as BenchmarkReport;
const reports = values.report!.map(read);
const baseline = read(values.baseline!);
process.stdout.write(`Against the base's build:\n${compared(reports.at(-1)!, baseline)}`);
checkBeside(reports, baseline);
