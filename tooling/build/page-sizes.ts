// `dispatchdev check`'s measure of the dashboard against its page-size budgets, before a push:
// the production build the artifact serves, alone, in the temporary folder. A change over a
// budget measures `origin/main` with `main`'s own copy of this script and its build, to say
// whether the budget was full before it.
// `--json` prints the checkout's sizes alone, as the measure of `main` reads them.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { buildDashboard } from './dashboard.js';
import {
  assetSizes,
  graphSizes,
  overBudget,
  pageSizeReport,
  type Sizes,
} from '../testing/page-budgets.js';

const temporary = (name: string) => fs.mkdtempSync(path.join(os.tmpdir(), `${name}-`));

/** The checkout's sizes, from a dashboard built in a temporary folder that goes after. */
async function measure(): Promise<Sizes> {
  const out = temporary('dashboard');
  try {
    const built = await buildDashboard(out, 'error');
    return { graphs: graphSizes(out, built), assets: assetSizes(out) };
  } finally {
    fs.rmSync(out, { recursive: true, force: true });
  }
}

/** `origin/main`'s sizes, measured from its committed tree by its own copy of this script, so its
 * own build and compression, with this checkout's packages; or why not: with other packages, the
 * sizes would be neither `main`'s nor this change's. */
function measureMain(): Sizes | string {
  const lock = execFileSync('git', ['show', 'origin/main:package-lock.json'], {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (lock !== fs.readFileSync('package-lock.json', 'utf8'))
    return "not measured, as its packages differ from this change's";
  const tree = temporary('main');
  try {
    const archive = path.join(tree, 'main.tar');
    execFileSync('git', ['archive', '--output', archive, 'origin/main']);
    execFileSync('tar', ['-xf', archive, '-C', tree]);
    const script = path.join(tree, 'tooling/build/page-sizes.ts');
    if (!fs.existsSync(script)) return 'not measured, as it has no size check of its own yet';
    fs.symlinkSync(path.resolve('node_modules'), path.join(tree, 'node_modules'));
    return JSON.parse(
      execFileSync(path.resolve('node_modules/.bin/tsx'), [script, '--json'], {
        cwd: tree,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'inherit'],
      }),
    );
  } finally {
    fs.rmSync(tree, { recursive: true, force: true });
  }
}

const change = await measure();
if (process.argv.includes('--json')) {
  process.stdout.write(JSON.stringify(change));
} else {
  const over = overBudget(change);
  process.stdout.write(pageSizeReport(change, over ? measureMain() : undefined).join('\n') + '\n');
  if (over) process.exitCode = 1;
}
