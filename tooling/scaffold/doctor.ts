import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { unwired } from './wire.js';

// `dispatchdev doctor`: whether a checkout is ready to make and change things, each check a line
// with what fixes it. Each check is a function of the checkout, in CHECKS: a new one is a line
// there.

export type Finding = { check: string; ok: boolean; fix?: string; detail?: string[] };
type Check = { name: string; run: (root: string) => Omit<Finding, 'check'> };

/** The marker a generator leaves where its author writes, as the anatomy rule finds it. */
export const MARKER = ['TODO', '(scaffold)'].join('');
const git = (root: string, args: string[]) =>
  spawnSync('git', args, { cwd: root, encoding: 'utf8' });
const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');

/** Every file Git sees in the checkout, committed or not. */
function tracked(root: string) {
  return git(root, ['ls-files', '-z', '--cached', '--others', '--exclude-standard'])
    .stdout.split('\0')
    .filter((file) => file && fs.existsSync(path.join(root, file)));
}
/** Every file holding the marker, as the anatomy rule finds them: the generators' own aside. */
export const marked = (root: string) =>
  tracked(root)
    .filter((file) => !file.startsWith('tooling/scaffold/') && !file.startsWith('tooling/tests/'))
    .filter((file) => /\.(rs|tsx?|js|sql|md|toml)$/.test(file))
    .filter((file) => read(root, file).includes(MARKER));

export const CHECKS: Check[] = [
  {
    name: 'packages',
    run: (root) => {
      const installed = path.join(root, 'node_modules/.package-lock.json');
      if (!fs.existsSync(installed)) return { ok: false, fix: 'npm ci' };
      type Lock = { packages: Record<string, { version?: string; optional?: boolean }> };
      const wanted = (JSON.parse(read(root, 'package-lock.json')) as Lock).packages;
      const have = (JSON.parse(fs.readFileSync(installed, 'utf8')) as Lock).packages;
      // Optional packages are other platforms' builds, never installed here.
      const stale = Object.entries(wanted)
        .filter(([name, entry]) => name.startsWith('node_modules/') && !entry.optional)
        .filter(([name, entry]) => have[name]?.version !== entry.version)
        .map(([name]) => name.slice('node_modules/'.length));
      return stale.length ? { ok: false, fix: 'npm ci', detail: stale.slice(0, 10) } : { ok: true };
    },
  },
  {
    name: 'wiring',
    run: (root) => {
      const stale = unwired(root);
      return stale.length
        ? { ok: false, fix: 'dispatchdev generate', detail: stale }
        : { ok: true };
    },
  },
  {
    name: 'Cargo.lock',
    run: (root) => {
      const locked = spawnSync(
        'cargo',
        ['metadata', '--locked', '--offline', '--no-deps', '--format-version', '1'],
        { cwd: root, encoding: 'utf8' },
      );
      return locked.status === 0
        ? { ok: true }
        : { ok: false, fix: 'cargo update --workspace --offline' };
    },
  },
  {
    name: 'scaffold markers',
    run: (root) => {
      const left = marked(root);
      return left.length
        ? { ok: false, fix: `write what each ${MARKER} asks for`, detail: left }
        : { ok: true };
    },
  },
  {
    name: 'origin/main',
    run: (root) => {
      const behind = git(root, ['rev-list', '--count', 'HEAD..origin/main']);
      const count = Number(behind.stdout.trim());
      return behind.status !== 0 || count === 0
        ? { ok: true }
        : { ok: false, fix: 'git rebase origin/main', detail: [`${count} commits behind`] };
    },
  },
  {
    name: 'disk',
    run: (root) => {
      const { bavail, bsize } = fs.statfsSync(root);
      const free = bavail * bsize;
      const gb = (free / 1e9).toFixed(1);
      return free > 10e9
        ? { ok: true, detail: [`${gb} GB free`] }
        : { ok: false, fix: 'dispatchdev finish a merged change', detail: [`${gb} GB free`] };
    },
  },
];

export const doctor = (root: string): Finding[] =>
  CHECKS.map(({ name, run }) => ({ check: name, ...run(root) }));

/** The findings as people read them: a line each, then what fixes those that failed. */
export function described(findings: Finding[]) {
  return findings
    .flatMap((finding) => [
      `${finding.ok ? 'ok  ' : 'FIX '}  ${finding.check}${finding.fix && !finding.ok ? `: ${finding.fix}` : ''}`,
      ...(finding.ok ? [] : (finding.detail ?? []).map((line) => `        ${line}`)),
    ])
    .join('\n');
}
