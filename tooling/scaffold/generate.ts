import { spawn, spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { stripVTControlCharacters } from 'node:util';
import { wire } from './wire.js';

// `dispatchdev generate`: writes everything generated from what the owners declare, after any
// change to a feature, collector, tool or slot. The wiring first (wire.ts): the app's and the
// MCP's lists of features and tools. Then one build of the app's module tests, in the checkout's
// own target folder so the next run reuses it, and one run of the modules that write: the API
// types in TypeScript (`export`) and the snapshots the app's tests hold the whole product to,
// such as the catalog and the databases. A second run, without writing, shows they settled. It
// names each file it changed, and after a failure, fixing it and running it again finishes.

/** The app's crate, which mounts its module tests from app/tests/backend/. */
const LIB = 'app/backend/lib.rs';
const TESTS = 'app/tests/backend';
/** What marks a module test that writes: a snapshot it rewrites, or the API types. */
const WRITES = /snapshot::(check|compare|updating)\(|DISPATCH_UPDATE_CONTRACTS/;
const CARGO = ['cargo', 'test', '--locked', '-p', 'dispatch-backend', '--lib'];
const UPDATE = { DISPATCH_UPDATE_CONTRACTS: '1', DISPATCH_UPDATE_SNAPSHOTS: '1' };
/** How much of a line of output a failure shows. */
const LONG = 200;
/** How often a long step says it is still going. */
const BEAT = 20_000;

export type Step = {
  name: string;
  ok: boolean;
  seconds: number;
  /** Its whole output. */
  log?: string;
  /** Why it failed, the lines of its output that say so. */
  why?: string[];
};
export type Generated = {
  ok: boolean;
  steps: Step[];
  /** Every file the run changed, by its path from the root. */
  changed: string[];
};

/** The app's module tests that write, by module: what one run with the update variables runs. */
export function writers(root: string) {
  const lib = fs.readFileSync(path.join(root, LIB), 'utf8');
  return [...lib.matchAll(/#\[path = "\.\.\/tests\/backend\/(\w+)\.rs"\]\s*mod (\w+);/g)]
    .filter(([, file]) =>
      WRITES.test(fs.readFileSync(path.join(root, TESTS, `${file}.rs`), 'utf8')),
    )
    .map(([, , module]) => module!)
    .sort();
}

/** Seconds as people read them: 45s, 3m 05s. */
export const duration = (seconds: number) =>
  seconds < 60
    ? `${seconds}s`
    : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, '0')}s`;

/** The lines of a failed command's output that say why, or its last lines. */
export function failure(log: string) {
  const signs = [
    'FAILED',
    'panicked at',
    'error[',
    'error:',
    'Error:',
    '  --> ',
    'left:',
    'right:',
    'AssertionError',
  ];
  const lines = stripVTControlCharacters(log).split('\n');
  const found: string[] = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed && signs.some((sign) => line.includes(sign)) && found.at(-1) !== trimmed)
      found.push(trimmed);
  }
  const shown = found.length ? found.slice(0, 30) : lines.slice(-25).map((line) => line.trimEnd());
  // An assertion's two sides can be whole files: their starts say which.
  return shown.map((line) => (line.length > LONG ? `${line.slice(0, LONG)}...` : line));
}

/** Where a checkout keeps what it makes outside itself: its change's scratch folder. */
function scratch(root: string) {
  const dir = `/tmp/dispatch-${path.basename(root)}`;
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  return dir;
}

/** Whether this user can run a command in a systemd scope of its own, as on Dev's machine. */
let scoped: boolean | undefined;
function scopes() {
  scoped ??=
    spawnSync('systemd-run', ['--user', '--scope', '--quiet', '--collect', 'true'], {
      stdio: 'ignore',
    }).status === 0;
  return scoped;
}
/**
 * A command run below Dev: a low CPU weight and first in line for the out-of-memory killer, so
 * Dev never starves. No memory ceiling, which only slows a build down.
 */
function below(command: string[]) {
  return scopes()
    ? [
        'systemd-run',
        '--user',
        '--scope',
        '--quiet',
        '--collect',
        '-p',
        'CPUWeight=20',
        '--',
      ].concat(['choom', '-n', '300', '--'], command)
    : ['nice', '-n', '10', ...command];
}

/** Runs a step's command in `root`, its output going to `log`, saying every while it still runs. */
async function run(
  name: string,
  command: string[],
  options: { root: string; log: string; env?: Record<string, string>; say: (text: string) => void },
): Promise<Step> {
  const started = Date.now();
  const out = fs.openSync(options.log, 'w', 0o600);
  const [program, ...args] = below(command);
  const child = spawn(program!, args, {
    cwd: options.root,
    env: { ...process.env, ...options.env },
    stdio: ['ignore', out, out],
  });
  const beat = setInterval(
    () => options.say(`  ... ${name} ${duration(Math.round((Date.now() - started) / 1000))}`),
    BEAT,
  );
  const code = await new Promise<number>((resolve) => {
    child.on('error', () => resolve(1));
    child.on('close', (status) => resolve(status ?? 1));
  });
  clearInterval(beat);
  fs.closeSync(out);
  const step: Step = {
    name,
    ok: code === 0,
    seconds: Math.round((Date.now() - started) / 1000),
    log: options.log,
  };
  if (!step.ok) step.why = failure(fs.readFileSync(options.log, 'utf8'));
  return step;
}

/** Every file Git sees changed in `root`, with what it holds, or nothing outside Git. */
function changes(root: string) {
  const status = spawnSync('git', ['status', '--porcelain', '-z', '-uall', '--no-renames'], {
    cwd: root,
    encoding: 'utf8',
  });
  const found = new Map<string, string>();
  if (status.status !== 0) return found;
  for (const entry of status.stdout.split('\0').filter(Boolean)) {
    const file = entry.slice(3);
    let held = 'gone';
    try {
      held = crypto
        .createHash('sha1')
        .update(fs.readFileSync(path.join(root, file)))
        .digest('hex');
    } catch {}
    found.set(file, held);
  }
  return found;
}

/** A step as a line reads it: `ok      9s  write`. */
export const line = (step: Step) =>
  `${step.ok ? 'ok  ' : 'FAIL'}  ${duration(step.seconds).padStart(7)}  ${step.name}`;

/** The run as people read it: a line a step, why one failed, then the files it changed. */
export function report(generated: Generated) {
  const lines: string[] = [];
  const failed = generated.steps.find((step) => !step.ok);
  if (generated.changed.length)
    lines.push('Generated:', ...generated.changed.map((file) => `  ${file}`));
  else if (!failed) lines.push('Nothing generated changed.');
  if (failed)
    lines.push(
      `Fix what failed and run dispatchdev generate again${failed.log ? `: ${failed.log}` : '.'}`,
    );
  return lines.join('\n');
}

/**
 * Writes everything generated in `root`, printing a line a step as it ends unless `quiet`, and
 * `say`ing that a long step still runs.
 */
export async function generate(
  root: string,
  options: { quiet?: boolean; say?: (text: string) => void } = {},
): Promise<Generated> {
  const say = options.say ?? ((text: string) => process.stdout.write(`${text}\n`));
  const before = changes(root);
  const steps: Step[] = [];
  const done = (step: Step) => {
    steps.push(step);
    if (!options.quiet) {
      say(line(step));
      for (const why of step.why ?? []) say(`      ${why}`);
    }
    return step.ok;
  };
  const end = (): Generated => {
    const after = changes(root);
    const changed = [...new Set([...before.keys(), ...after.keys()])]
      .filter((file) => before.get(file) !== after.get(file))
      .sort();
    return { ok: steps.every((step) => step.ok), steps, changed };
  };

  const started = Date.now();
  try {
    wire(root);
    done({ name: 'wire', ok: true, seconds: Math.round((Date.now() - started) / 1000) });
  } catch (error) {
    done({
      name: 'wire',
      ok: false,
      seconds: Math.round((Date.now() - started) / 1000),
      why: [error instanceof Error ? error.message : String(error)],
    });
    return end();
  }
  const logs = scratch(root);
  const filters = writers(root).map((module) => `${module}::`);
  const step = (name: string, command: string[], env?: Record<string, string>) =>
    run(name, command, { root, log: path.join(logs, `generate-${name}.log`), env, say });
  if (!done(await step('build', [...CARGO, '--no-run']))) return end();
  // A test can read a snapshot another writes in the same pass, and fail on what it read
  // before: only the check after a pass says whether they agree, and a second pass settles them.
  for (let attempt = 0; attempt < 2; attempt++) {
    const write = await step('write', [...CARGO, '--', ...filters], UPDATE);
    done({ ...write, ok: true, why: undefined });
    const check = await step('check', [...CARGO, '--', ...filters]);
    if (check.ok || attempt === 1) {
      done(check);
      break;
    }
  }
  return end();
}
