import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { generator as piece } from './add.js';
import { KINDS } from './cli.js';
import { duration, failure, generate } from './generate.js';
import { make, repositoryRoot as root } from './scaffold.js';

// `npm run check:scaffold`: what every generator writes builds and passes the rules as written.
// In a copy of the checkout it makes one of each kind with most of its options, as `dispatchdev
// new` would, and writes what is generated from them; then it holds the copy to clippy,
// TypeScript, the linter, the rules and the new owners' own tests. The copy builds in this
// checkout's target folder, so the dependencies' builds are reused. `--keep` keeps the copy.
// The CI scaffold job runs it.

/** What it makes, each as `dispatchdev new <kind> ...` would: the order matters, a keeper needs
 * its collector first and a tool its feature. */
const MADE: [string, ...string[]][] = [
  ['collector', 'smokesite'],
  ['feature', 'smoke_keeper', '--preset', 'keeper', '--keeps', 'smokesite.records'],
  // Each other preset.
  ['feature', 'smoke_tabs', '--preset', 'page-with-tabs'],
  ['feature', 'smoke_connection', '--preset', 'connection'],
  ['feature', 'smoke_source', '--preset', 'people-source'],
  ['feature', 'smoke_settings', '--preset', 'settings', '--mandatory'],
  [
    'feature',
    'smoke_people',
    '--tables',
    'dsp',
    '--people',
    'amazon',
    '--settings',
    '--icon',
    'Truck',
  ],
  // A tab of a page without tabs of its own, and of one with them.
  ['feature', 'smoke_tab', '--tab-of', 'uniforms'],
  ['feature', 'smoke_team_tab', '--tab-of', 'team', '--mandatory'],
  ['feature', 'smoke_front', '--no-backend', '--page'],
  // What one exposes, which a page shows, and a tool reading it.
  ['feature', 'smoke_offer', '--contract', 'smoke_counts', '--api', '--shown'],
  ['feature', 'smoke_always', '--mandatory', '--contract', 'smoke_always_counts'],
  [
    'tool',
    'smoke_lookup',
    '--features',
    'smoke_people',
    '--scope',
    'dsps',
    '--actions',
    'list,flag',
    '--changing',
    'flag',
  ],
  ['tool', 'smoke_ping', '--scope', 'connection'],
  ['tool', 'smoke_reader', '--reads', 'smoke_offer'],
  // A mandatory feature's, which no test can switch.
  ['tool', 'smoke_always_reader', '--reads', 'smoke_always'],
  // One of Slack's own integrations, and one of another feature's through Slack.
  ['integration', 'smoke_alerts', '--needs', 'chat:write', '--hears', 'message'],
  ['integration', 'smoke_notes', '--to', 'smoke_people'],
  ['slot', 'smoke_badges', '--description', 'Its badges on the DSPs page.'],
  // A second slot, after the first.
  ['slot', 'smoke_shortcuts', '--description', 'Its shortcuts on a page.'],
  // Features the pieces below are added to: one with a table, and one of a manifest alone,
  // whose test module is its manifest's last item.
  ['feature', 'smoke_rows', '--tables', 'dsp'],
  ['feature', 'smoke_bare', '--mandatory'],
];

/**
 * Pieces added to features that exist, after the app's people test has read what the features
 * above declare: an added piece's people read a table the author has yet to fit them to.
 */
const ADDED: string[][] = [
  ['people', 'paycom', '--to', 'smoke_rows', '--table', 'smoke_rows_items'],
  ['tab-of', 'team', '--to', 'smoke_rows'],
  ['page', '--to', 'smoke_people'],
  ['api', '--to', 'smoke_front'],
  ['api', '--to', 'smoke_bare'],
  ['contract', 'smoke_rows_counts', '--to', 'smoke_rows'],
  // The product's own: Team has no backend/ for its settings, nor a team.view for their manage
  // permission to imply, which add once broke; Uniform Inventory's settings join the backend
  // and tests it has. Each case after them is one add once broke: Timecard keeps its people's
  // table in Paycom's database and declares a module without pub(crate); Weekly Scorecard's
  // retired list names its permissions; Documents imports through a nested use tree.
  ['settings', '--to', 'team'],
  ['settings', '--to', 'uniforms'],
  ['people', 'paycom', '--to', 'timecard', '--table', 'employees'],
  ['page', '--to', 'weekly_scorecard'],
  ['people', 'amazon', '--to', 'documents', '--table', 'documents_files'],
  // A keeper's tests install the collector it keeps from, as its contract's must; a page shows
  // it.
  ['contract', 'keeper_counts', '--to', 'smoke_keeper', '--shown'],
];

/** Copies the checkout's files, committed or not, into `to`, with its packages linked. */
function copy(to: string) {
  const listed = spawnSync('git', ['ls-files', '-z', '-co', '--exclude-standard'], {
    cwd: root,
    encoding: 'utf8',
  }).stdout;
  for (const file of listed.split('\0').filter(Boolean)) {
    const from = path.join(root, file);
    if (!fs.existsSync(from)) continue;
    fs.mkdirSync(path.dirname(path.join(to, file)), { recursive: true });
    fs.copyFileSync(from, path.join(to, file));
    // Its time too, so the next run's build reuses what this one built of an unchanged file.
    const { atime, mtime } = fs.statSync(from);
    fs.utimesSync(path.join(to, file), atime, mtime);
  }
  fs.symlinkSync(path.join(root, 'node_modules'), path.join(to, 'node_modules'));
  // The rules and the report of what was generated read the files through Git.
  spawnSync('git', ['init', '-q'], { cwd: to });
}

/** Runs a step's command in the copy, a line when it ends, and the lines saying why it failed. */
async function step(name: string, command: string[], cwd: string, logs: string) {
  const started = Date.now();
  const log = path.join(logs, `${name.replace(/\W+/g, '-')}.log`);
  const out = fs.openSync(log, 'w');
  const child = spawn(command[0]!, command.slice(1), { cwd, stdio: ['ignore', out, out] });
  const ok = await new Promise<boolean>((resolve) => {
    child.once('error', () => resolve(false));
    child.once('exit', (code) => resolve(code === 0));
  });
  fs.closeSync(out);
  const took = duration(Math.round((Date.now() - started) / 1000)).padStart(7);
  process.stdout.write(`${ok ? 'ok  ' : 'FAIL'}  ${took}  ${name}\n`);
  if (!ok)
    for (const line of [...failure(fs.readFileSync(log, 'utf8')), `Whole output: ${log}`])
      process.stdout.write(`      ${line}\n`);
  return ok;
}

/** Makes one of each kind in a copy at `copied`, then holds it to every check, in order. */
async function check(copied: string, logs: string) {
  copy(copied);
  const say = (text: string) => process.stdout.write(`${text}\n`);
  for (const [kind, ...args] of MADE) {
    await make(
      KINDS.find((each) => each.kind === kind)!,
      [...args, '--root', copied],
    );
    say(`ok            new ${kind} ${args.join(' ')}`);
  }
  process.env.CARGO_TARGET_DIR ??= path.join(root, 'target');
  if (!(await generate(copied, { say })).ok) return false;
  // The app's people test reads every kind of data from a new DSP: the queries written above.
  const people = ['cargo', 'test', '--locked', '-p', 'dispatch-backend', '--lib', '--', 'people::'];
  if (!(await step('people', people, copied, logs))) return false;
  for (const args of ADDED) {
    await make(piece, [...args, '--root', copied]);
    say(`ok            add ${args.join(' ')}`);
  }
  if (!(await generate(copied, { say })).ok) return false;
  const tsx = path.join(root, 'node_modules/tsx/dist/cli.mjs');
  const plan = JSON.parse(
    fs.readFileSync(path.join(copied, 'tooling/ci/test-plan.json'), 'utf8'),
  ) as { rules: string[] };
  // Every architecture rule, found by where it lives, and the tooling's that the plan lists.
  const rules = [
    ...fs
      .readdirSync(path.join(copied, 'app/tests/rules'))
      .filter((name) => name.endsWith('.test.ts'))
      .sort()
      .map((name) => `app/tests/rules/${name}`),
    ...plan.rules.filter((file) => file !== 'tooling/tests/scaffold.test.ts'),
  ];
  const owners = ['features', 'collectors', 'mcp/tools']
    .flatMap((dir) => fs.readdirSync(path.join(copied, dir)).map((name) => `${dir}/${name}`))
    .filter((dir) => /\/smoke/.test(dir));
  const crates = owners
    .filter((dir) => fs.existsSync(path.join(copied, dir, 'Cargo.toml')))
    .map((dir) => `dispatch-${path.basename(dir).replace(/_/g, '-')}`);
  const steps: [string, string[]][] = [
    [
      'clippy',
      ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--', '-D', 'warnings'],
    ],
    [
      'TypeScript',
      [process.execPath, 'node_modules/typescript/bin/tsc', '--noEmit', '-p', 'tsconfig.json'],
    ],
    [
      'lint',
      [
        process.execPath,
        'node_modules/eslint/bin/eslint.js',
        // Each with TypeScript to lint: a feature of a contract alone has none.
        ...owners.filter(
          (dir) =>
            !dir.startsWith('mcp/') &&
            fs
              .readdirSync(path.join(copied, dir), { recursive: true, encoding: 'utf8' })
              .some((file) => /\.tsx?$/.test(file)),
        ),
      ],
    ],
    // What a generator leaves its author to write is there on purpose, and a new browser test
    // has no recorded time yet. The generators' own tests run on the real tree in the checks.
    [
      'rules',
      [
        process.execPath,
        tsx,
        '--test',
        '--test-skip-pattern=generator left',
        '--test-skip-pattern=the recorded times name the suite',
        ...rules,
      ],
    ],
    ['owner tests', ['cargo', 'test', '--locked', ...crates.flatMap((crate) => ['-p', crate])]],
    ['tool tests', ['cargo', 'test', '--locked', '-p', 'dispatch-mcp', '--', 'smoke_']],
  ];
  for (const [name, command] of steps) if (!(await step(name, command, copied, logs))) return false;
  return true;
}

// One place for every run, which the build's records name, so a run after it builds only what
// changed.
const at = path.join(os.tmpdir(), 'dispatch-scaffold-check');
fs.rmSync(at, { recursive: true, force: true });
fs.mkdirSync(at, { recursive: true });
const logs = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-scaffold-logs-'));
try {
  if (!(await check(path.join(at, 'checkout'), logs))) process.exitCode = 1;
} finally {
  if (process.argv.includes('--keep')) process.stdout.write(`Kept the copy in ${at}/checkout.\n`);
  else fs.rmSync(at, { recursive: true, force: true });
}
