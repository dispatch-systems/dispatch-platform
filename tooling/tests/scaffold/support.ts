import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { planFeature } from '../../scaffold/new-feature.js';
import { repositoryRoot as root, type Plan } from '../../scaffold/scaffold.js';

// What the generators' tests share. They run the generators with --dry-run: the files each flag
// writes, what they say, and how the new owner is listed in app/. Nothing here builds Rust; the
// output is checked as text.

export { root };
export const temporary = () => fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-scaffold-'));
export const feature = async (...argv: string[]) => (await planFeature(root, argv)).plan;
/** The files a plan writes under `dir`, relative to it. */
export const writes = (plan: Plan, dir: string) =>
  [...plan.files.keys()].map((file) => path.posix.relative(dir, file)).sort();
export const file = (plan: Plan, name: string) => {
  const content = plan.files.get(name) ?? plan.changes.get(name);
  assert(content !== undefined, `the plan has no ${name}`);
  return content;
};
export const changed = (plan: Plan, name: string) => {
  const content = plan.changes.get(name);
  assert(content !== undefined, `the plan changes no ${name}`);
  return content;
};
export const refused = async (pattern: RegExp, ...argv: string[]) =>
  assert.rejects(planFeature(root, argv), pattern);
export const registry = ['app/backend/features.rs', 'app/backend/lib.rs'].find(
  (candidate) =>
    fs.existsSync(candidate) && /pub static REGISTRY\b/.test(fs.readFileSync(candidate, 'utf8')),
)!;

/**
 * A copy of the repository's workspace, for what needs a collector that exists only there, or
 * changes what a generator reads. Only a file whose tests write into it makes one.
 */
export function copyWorkspace() {
  const copy = temporary();
  for (const entry of [
    'Cargo.toml',
    'Cargo.lock',
    'rust-toolchain.toml',
    'app',
    'core',
    'collectors',
    'features',
    'mcp',
    'ops/host-manager',
    'tooling/cli',
    'tooling/shared',
    'services/support-worker',
    'tooling/ci/test-plan.json',
    '.github/workflows/checks.yml',
  ])
    fs.cpSync(entry, path.join(copy, entry), {
      recursive: true,
      filter: (source) => !/(^|\/)(node_modules|target)(\/|$)/.test(source),
    });
  return copy;
}
