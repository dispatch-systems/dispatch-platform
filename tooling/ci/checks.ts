import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { assessmentFixture } from '../testing/ci-tools.js';
import { PAINT_BUDGET, WORKERS, browserTests, shards } from './browser-shards.js';
import { nodeTests, pythonTests, sourceLintCommands, type Command } from './execution-plan.js';

// One job of the platform checks, or locally the whole suite in sequence. CI runs each mode
// on its own runner: `build` packages the runtime, and the modes that need it download that
// package into `.build` first.
const mode = process.argv[2] ?? 'full';
const modes = ['full', 'build', 'checks', 'browser', 'core', 'api', 'benchmark', 'smoke'];
if (!modes.includes(mode)) throw new Error('Unknown validation mode');
const started = Date.now();
const failures: string[] = [];
const dashboardCommand = nodeTests('dashboard', {
  concurrency: 1,
  env: { DISPATCH_TEST_BINARY: '.build/services/rust/dispatch-backend' },
});
const coreCommand = nodeTests('core', { concurrency: os.availableParallelism() });
const checkCommands: Command[] = [
  { name: 'check:privacy', command: 'npm', args: ['run', 'check:privacy'] },
  { name: 'check', command: 'npm', args: ['run', 'check'] },
  { name: 'format:check', command: 'npm', args: ['run', 'format:check'] },
  { name: 'test:artifact', command: 'npm', args: ['run', 'test:artifact'] },
  ...sourceLintCommands,
  dashboardCommand,
];
// Coverage tests inspect the same commands the runners execute, without starting servers,
// compiling Rust, downloading browsers or reaching the dependency-audit service.
if (process.argv.includes('--list')) {
  const commands =
    mode === 'checks' ? checkCommands : mode === 'api' ? [pythonTests, coreCommand] : [];
  if (!['checks', 'api'].includes(mode))
    throw new Error('Command listing requires checks or api mode');
  process.stdout.write(`${JSON.stringify(commands)}\n`);
  process.exit(0);
}
async function run(name: string, command: string, args: string[], env = process.env) {
  const start = Date.now();
  process.stdout.write(`[start] ${name}\n`);
  const child = spawn(command, args, { stdio: 'inherit', env });
  const ok = await new Promise<boolean>((resolve) => {
    child.once('error', () => resolve(false));
    child.once('exit', (code) => resolve(code === 0));
  });
  process.stdout.write(
    `[${ok ? 'pass' : 'fail'}] ${name} (${((Date.now() - start) / 1000).toFixed(1)}s)\n`,
  );
  if (!ok) failures.push(name);
  return ok;
}
const npm = (name: string, ...args: string[]) =>
  run(name, 'npm', ['run', name, ...(args.length ? ['--', ...args] : [])]);
const execute = ({ name, command, args, env }: Command) =>
  run(name, command, args, env ? { ...process.env, ...env } : process.env);
/** Source privacy, types, formatting, bundle budget and dashboard logic against `.build`. */
function checks() {
  return Promise.all(checkCommands.map(execute));
}
/** The workload regression, against the build already in `.build`. */
function benchmark() {
  return run('Rust workload regression', process.execPath, [
    'node_modules/tsx/dist/cli.mjs',
    'tooling/benchmarks/benchmark-rust.ts',
    '--binary',
    '.build/services/rust/dispatch-backend',
    '--check',
    '--output',
    '/tmp/dispatch-rust-benchmark.json',
  ]);
}
/**
 * The pinned browser, from the cache when the job restored it. In CI its system packages
 * install too, unless the job installs them in the background and names that install in
 * DISPATCH_BROWSER_PACKAGES: then this waits for its result.
 */
async function installBrowsers() {
  const packages = process.env.DISPATCH_BROWSER_PACKAGES;
  const browser = run('browser setup', 'npx', [
    'playwright',
    'install',
    ...(process.env.CI === 'true' && !packages ? ['--with-deps'] : []),
    'chromium',
  ]);
  if (!packages) return browser;
  const start = Date.now();
  // The install writes its exit status last; five minutes without one is a hung install.
  while (!fs.existsSync(`${packages}.status`) && Date.now() - start < 300000)
    await new Promise((resolve) => setTimeout(resolve, 250));
  const installed =
    fs.existsSync(`${packages}.status`) &&
    fs.readFileSync(`${packages}.status`, 'utf8').trim() === '0';
  const log = fs.existsSync(`${packages}.log`) ? fs.readFileSync(`${packages}.log`, 'utf8') : '';
  if (!installed) {
    process.stdout.write(log);
    failures.push('browser system packages');
  } else {
    // What installed and how long it took, so a slow mirror shows in a passing run too.
    for (const line of log.split('\n'))
      if (/^(Installing |Installed |Fetched |\d+ upgraded)/.test(line))
        process.stdout.write(`${line}\n`);
  }
  process.stdout.write(
    `[${installed ? 'pass' : 'fail'}] browser system packages, waited ${((Date.now() - start) / 1000).toFixed(1)}s\n`,
  );
  return (await browser) && installed;
}
/**
 * One shard of the browser suite against the build already in `.build`, as `browser 3/6`.
 * A manual lane names a spec or test after the shard; shards it leaves empty still pass.
 */
async function browser() {
  const shard = /^([1-9]\d*)\/([1-9]\d*)$/.exec(process.argv[3] ?? '');
  if (!shard || Number(shard[1]) > Number(shard[2]))
    throw new Error('Browser shard required, as 1/6');
  const [index, count] = [Number(shard[1]), Number(shard[2])];
  const only = process.argv.slice(4).filter(Boolean);
  // The browser downloads and installs its system packages while Cargo compiles the
  // fixture; test:ui then finds the fixture already built.
  const fixture = assessmentFixture(process.env, process.cwd())
    ? Promise.resolve(true)
    : run('assessment fixture', 'cargo', ['build', '--locked', '--example', 'assessment-fixture']);
  const setup = Promise.all([installBrowsers(), fixture]);
  // Meanwhile, this shard's share of a split by the tests' recorded times, which every shard
  // computes alike: Playwright's own sharding splits by count, and one long test made one
  // shard the slowest of every run.
  const tests = shards(browserTests(), count)[index - 1]!;
  const list = path.join(os.tmpdir(), `dispatch-browser-shard-${index}-of-${count}.txt`);
  fs.writeFileSync(list, `${tests.join('\n')}\n`);
  if (!(await setup).every(Boolean)) return;
  // Paint budgets run alone after the rest, with their own output so the first run's traces
  // survive.
  const alone = tests.some((test) => test.includes(PAINT_BUDGET));
  // Three workers on a four-core runner: the fourth core keeps the private servers and the
  // sign-in animation responsive, so long multi-login tests stay well inside their budget.
  await npm(
    'test:ui',
    '--test-list',
    list,
    `--workers=${WORKERS}`,
    ...(alone ? ['--grep-invert', PAINT_BUDGET] : []),
    ...(only.length ? ['--pass-with-no-tests', ...only] : []),
  );
  if (alone) {
    const output = process.env.DISPATCH_TEST_OUTPUT ?? 'test-results';
    await run(
      'paint budgets',
      'npm',
      [
        ...['run', 'test:ui', '--', '--test-list', list, '--workers=1'],
        ...['--grep', PAINT_BUDGET, '--pass-with-no-tests', ...only],
      ],
      { ...process.env, DISPATCH_TEST_OUTPUT: path.join(output, 'paint-budgets') },
    );
  }
}
/** Rust formatting, lints and tests: `npm run check:rust` compiles what it checks. */
function core() {
  return npm('check:rust');
}
/** The API tests against a debug backend, with the Python tooling tests and the npm audit. */
async function api() {
  const python = execute(pythonTests);
  const audit = run('dependency audit', 'npm', ['audit', '--audit-level=high']);
  if (await run('debug build', 'python3', ['tooling/cargo-build.py'])) {
    await execute(coreCommand);
  }
  await Promise.all([python, audit]);
}
if (mode === 'build') await npm('build');
else if (mode === 'checks') await checks();
else if (mode === 'browser') await browser();
else if (mode === 'core') await core();
else if (mode === 'api') await api();
else if (mode === 'benchmark') await benchmark();
else if (mode === 'smoke') await npm('test:smoke');
else {
  // Locally, in sequence: CI's jobs share one machine here, and Cargo's build lock.
  await core();
  if (!failures.length) await api();
  if (!failures.length) await npm('build');
  if (!failures.length) await checks();
  if (!failures.length && (await installBrowsers())) await npm('test:ui');
  if (!failures.length) await benchmark();
  if (!failures.length) await npm('test:browseros');
}
process.stdout.write(`Validation ${mode}: ${((Date.now() - started) / 1000).toFixed(1)}s\n`);
if (failures.length) {
  process.stderr.write(`Failed checks: ${failures.join(', ')}\n`);
  process.exitCode = 1;
}
