import test from 'node:test';
import { featureManifest, textOf } from './support/manifests.js';
import { holds } from './support/holds.js';
import {
  collectors,
  entries,
  features,
  files,
  filesIn,
  isDirectory,
  isFile,
  owners,
} from './support/repo.js';
import { lexFile, testAttributes } from './support/rust.js';

// What every feature, collector and core part is made of: plans/restructure/structure.md,
// "A feature's anatomy" and "What every feature has, and what it adds".
const featurePieces = ['backend', 'api', 'mcp', 'frontend', 'migrations', 'tooling', 'tests'];
const collectorPieces = [
  'connection',
  'discovery',
  'collections',
  'scripts',
  'fixtures',
  'frontend',
  'migrations',
  'probes',
  'tests',
];

/**
 * Whether a collector's api/ holds only generated/: it serves no endpoints, but the TypeScript
 * of its types that features answer with is written there (enforcement.md, section 2).
 */
const generatedOnly = (dir: string) => entries(`${dir}/api`).every((name) => name === 'generated');

/** Whether a folder holds a Rust test: a `#[test]` in a file under it. */
const hasRustTest = (directory: string) =>
  filesIn(directory).some((file) => file.endsWith('.rs') && testAttributes(lexFile(file)).length);

test('every feature has its crate, manifest and README, and a backend, an API or a frontend', () => {
  const missing = features().flatMap(({ dir }) => [
    ...['Cargo.toml', 'feature.rs', 'README.md']
      .filter((file) => !isFile(`${dir}/${file}`))
      .map((file) => `${dir} has no ${file}`),
    ...(['backend', 'api', 'frontend'].some((piece) => isDirectory(`${dir}/${piece}`))
      ? []
      : [`${dir} has no backend/, api/ or frontend/`]),
  ]);
  holds('anatomy', 'feature minimum', missing);
});

// Its screens and routes check permissions; a feature that can be switched on with none
// would have nothing to check them against. Every DSP has a mandatory one, switched or not.
test('an optional feature declares a permission', () => {
  const unchecked = features().flatMap((owner) => {
    const manifest = featureManifest(owner);
    return manifest?.switch && !manifest.mandatory && !manifest.permissions.length
      ? [`${owner.dir} is optional and has no permission`]
      : [];
  });
  holds('anatomy', 'switch without permission', unchecked);
});

test('an optional piece arrives with its companions', () => {
  const alone = features().flatMap((owner) => {
    const { dir } = owner;
    const out: string[] = [];
    if (isDirectory(`${dir}/backend`) && !hasRustTest(`${dir}/tests/backend`))
      out.push(`${dir} has backend/ and no test in tests/backend/`);
    if (isDirectory(`${dir}/api`) && !isDirectory(`${dir}/tests/api`))
      out.push(`${dir} has api/ and no tests/api/`);
    // The tools agents use come with tests of their own.
    if (isDirectory(`${dir}/mcp`) && !hasRustTest(`${dir}/tests/backend/mcp`))
      out.push(`${dir} has mcp/ and no test in tests/backend/mcp/`);
    if (isDirectory(`${dir}/frontend`)) {
      if (!isFile(`${dir}/frontend/feature.ts`))
        out.push(`${dir} has frontend/ and no frontend/feature.ts`);
      if (!isDirectory(`${dir}/tests/browser`))
        out.push(`${dir} has frontend/ and no tests/browser/`);
    }
    // Tables arrive with the migrations that create them, and migrations with their
    // declaration in the manifest.
    const manifest = featureManifest(owner);
    const declares = (field: string) => {
      const span = manifest?.fields.get(field);
      return !!span && !/^&\s*\[\s*\]$/.test(textOf(span));
    };
    const migrates = declares('migrations') || declares('own_migrations');
    if (declares('tables') && !(migrates && isDirectory(`${dir}/migrations`)))
      out.push(`${dir} declares tables and no migrations`);
    if (isDirectory(`${dir}/migrations`) && !migrates)
      out.push(`${dir} has migrations/ its manifest does not declare`);
    return out;
  });
  holds('anatomy', 'companions', alone);
});

test("a feature's and a collector's root hold only their pieces", () => {
  const stray = [
    ...features().flatMap(({ dir }) =>
      entries(dir)
        .filter(
          (name) => ![...featurePieces, 'README.md', 'Cargo.toml', 'feature.rs'].includes(name),
        )
        .map((name) => `${dir}/${name} is no piece of a feature`),
    ),
    ...collectors().flatMap(({ dir }) => [
      ...['Cargo.toml', 'collector.rs', 'README.md']
        .filter((file) => !isFile(`${dir}/${file}`))
        .map((file) => `${dir} has no ${file}`),
      ...entries(dir)
        .filter(
          (name) => ![...collectorPieces, 'README.md', 'Cargo.toml', 'collector.rs'].includes(name),
        )
        .filter((name) => name !== 'api' || !generatedOnly(dir))
        .map((name) => `${dir}/${name} is no piece of a collector`),
    ]),
  ];
  holds('anatomy', 'roots', stray);
});

// The MCP is one owner in one folder: its crate, its README, and the pieces a feature may
// have, each with its tests.
const mcpPieces = ['backend', 'api', 'frontend', 'migrations', 'tests'];
test("the MCP's root holds its crate, its README and its pieces, each with its tests", () => {
  const out = [
    ...['Cargo.toml', 'mcp.rs', 'README.md']
      .filter((file) => !isFile(`mcp/${file}`))
      .map((file) => `mcp has no ${file}`),
    ...entries('mcp')
      .filter((name) => ![...mcpPieces, 'Cargo.toml', 'mcp.rs', 'README.md'].includes(name))
      .map((name) => `mcp/${name} is no piece of the MCP`),
  ];
  if (isDirectory('mcp/backend') && !hasRustTest('mcp/tests/backend'))
    out.push('mcp has backend/ and no test in tests/backend/');
  if (isDirectory('mcp/api') && !isDirectory('mcp/tests/api'))
    out.push('mcp has api/ and no tests/api/');
  if (isDirectory('mcp/frontend') && !isFile('mcp/frontend/feature.ts'))
    out.push('mcp has frontend/ and no frontend/feature.ts');
  if (isDirectory('mcp/frontend') && !isDirectory('mcp/tests/browser'))
    out.push('mcp has frontend/ and no tests/browser/');
  holds('anatomy', 'the MCP', out);
});

// Core is one crate: its parts are folders named as a feature's pieces, with no crate or
// root of their own, beside core's Cargo.toml and core.rs.
test('core holds its parts, and each part only the folders a feature may have', () => {
  const stray = [
    ...entries('core')
      .filter((name) => !isDirectory(`core/${name}`))
      .filter((name) => !['Cargo.toml', 'core.rs', 'README.md'].includes(name))
      .map((name) => `core/${name} is no part of core`),
    ...owners('core').flatMap(({ dir }) => [
      ...(isFile(`${dir}/README.md`) ? [] : [`${dir} has no README.md`]),
      ...entries(dir)
        .filter((name) => ![...featurePieces, 'README.md'].includes(name))
        .map((name) => `${dir}/${name} is no folder a feature may have`),
    ]),
  ];
  holds('anatomy', 'core parts', stray);
});

test("each tab sits in its feature's frontend/tabs/, and each folder there is a tab", () => {
  const misplaced: string[] = [];
  for (const owner of features()) {
    const manifest = featureManifest(owner);
    const declared = (manifest?.tabs ?? []).map((id) =>
      id.slice(id.indexOf('.') + 1).replaceAll('_', '-'),
    );
    const folders = entries(`${owner.dir}/frontend/tabs`).filter((name) =>
      isDirectory(`${owner.dir}/frontend/tabs/${name}`),
    );
    for (const tab of declared)
      if (!folders.includes(tab)) misplaced.push(`${owner.dir} has no frontend/tabs/${tab}/`);
    for (const folder of folders)
      if (!declared.includes(folder))
        misplaced.push(`${owner.dir}/frontend/tabs/${folder} is no tab its manifest declares`);
  }
  // Tabs are a feature's page's own.
  for (const file of files)
    if (/(^|\/)tabs\//.test(file) && !/^features\/[a-z_]+\/frontend\/tabs\//.test(file))
      misplaced.push(
        `${file.slice(0, file.indexOf('tabs/') + 4)} is outside a feature's frontend/`,
      );
  holds('anatomy', 'tabs', misplaced);
});
