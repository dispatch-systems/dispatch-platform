import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { workspace } from './support/cargo.js';
import { files, isFile, read } from './support/repo.js';
import { lex } from './support/rust.js';

// What every installed host expects of a build, whatever its own version: the dashboard's
// document, the backend binary and the build's description. Installed hosts run their own
// copies of the host manager and launchers, never this repository's, so these never change.
const required = [
  'dashboard/index.html',
  'services/rust/dispatch-backend',
  'tooling/build-info.json',
];
const binary = 'dispatch-backend';

test('the build writes the layout the installed hosts expect', () => {
  const build = read('tooling/build/build.ts');
  // The backend binary is copied from Cargo's release folder to its place.
  assert.match(build, new RegExp(`'release/${binary}'`));
  assert.match(build, /'services\/rust\/dispatch-backend'/);
  // Vite writes the dashboard, its index.html included, into dashboard/.
  assert.match(build, /outDir:\s*path\.join\(staging,\s*'dashboard'\)/);
  // The build's description sits in tooling/.
  assert.match(build, /'tooling\/build-info\.json'/);
  assert.match(build, /writeManifest\(staging,/);
});

test('the dashboard is built from an index.html, as dashboard/index.html', () => {
  const config = read('vite.config.ts');
  const rootDirectory = /\broot:\s*'([^']+)'/.exec(config)?.[1];
  assert(rootDirectory, 'vite.config.ts names the dashboard root');
  assert(
    isFile(`${rootDirectory}/index.html`),
    `${rootDirectory}/index.html is the dashboard's document`,
  );
  const outDir = /\boutDir:\s*'([^']+)'/.exec(config)?.[1];
  assert.equal(outDir && path.posix.join(rootDirectory, outDir), '.build/dashboard');
});

test(`one crate of the workspace builds the ${binary} binary`, () => {
  const builders = workspace().filter((crate) =>
    crate.targets.some(
      (target) => target.kind === 'bin' && target.name === binary.replaceAll('-', '_'),
    ),
  );
  assert.equal(builders.length, 1, `exactly one crate builds ${binary}`);
  assert.equal(builders[0]!.name, binary, `the crate that builds it is ${binary}`);
});

test("this repository's host manager and units expect the same layout", () => {
  const manager = lex(read('ops/host-manager/src/artifact.rs'));
  const list = /\bconst\s+REQUIRED\s*:[^=]*=\s*\[/.exec(manager.masked);
  assert(list, 'the host manager lists the files a build must have');
  const close = manager.masked.indexOf(']', list.index + list[0].length);
  const listed = manager.literals
    .filter((literal) => literal.start > list.index && literal.end <= close)
    .map((literal) => literal.value);
  assert.deepEqual(listed.sort(), [...required].sort());
  // The units start the binary from the installed build.
  const units = files.filter((file) =>
    /^ops\/systemd\/dispatch-(dev|production(-isolated)?)\.service$/.test(file),
  );
  assert(units.length >= 3, 'the Dev and Production units exist');
  for (const unit of units)
    assert.match(
      read(unit),
      /\/services\/rust\/dispatch-backend serve\b/,
      `${unit} starts the installed binary`,
    );
});
