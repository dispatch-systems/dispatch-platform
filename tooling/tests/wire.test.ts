import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  crateOf,
  crates,
  mcpMarker,
  toolOf,
  tools,
  wiredManifest,
  wiredMcpManifest,
  wiredProduct,
  wiredTools,
  type Crate,
} from '../scaffold/wire.js';

// The MCP's wiring, from its tools: what `npm run contracts:generate` writes so that a tool
// needing a feature can use its crate, in builds that have it and no other.

const features: Crate[] = [
  crateOf(
    'dvic',
    '[package]\nname = "dispatch-dvic"\n\n[dependencies]\ndispatch-core = { path = "../../core" }\ndispatch-cortex = { path = "../../collectors/cortex" }\n',
  ),
  crateOf(
    'driver_match',
    '[package]\nname = "dispatch-driver-match"\n\n[dependencies]\ndispatch-core = { path = "../../core" }\n',
  ),
  crateOf(
    'timecard',
    '[package]\nname = "dispatch-timecard"\n\n[dependencies]\ndispatch-driver-match = { path = "../driver_match" }\ndispatch-cortex = { path = "../../collectors/cortex" }\ndispatch-paycom = { path = "../../collectors/paycom" }\n',
  ),
];
const tool = (module: string, needs: string) =>
  toolOf(
    module,
    `pub struct T;\nimpl Tool for T {\n    const FEATURES: &'static [&'static str] = &[${needs}];\n}\n`,
    features,
  );
/** A workspace with only the collectors' manifests, which name their crates. */
function workspace() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-wire-'));
  for (const name of ['cortex', 'paycom']) {
    fs.mkdirSync(path.join(root, 'collectors', name), { recursive: true });
    fs.writeFileSync(
      path.join(root, 'collectors', name, 'Cargo.toml'),
      `[package]\nname = "dispatch-${name}"\n`,
    );
  }
  return root;
}

test('a tool needs its features without their parts, and only features there are', () => {
  assert.deepEqual(tool('review', '"timecard.meal_breaks", "dvic", "timecard"').needs, [
    'dvic',
    'timecard',
  ]);
  assert.deepEqual(tool('whoami', '').needs, []);
  assert.equal(toolOf('x', 'impl Tool for Lookup {}', features).type, 'Lookup');
  assert.throws(() => tool('beam', '"teleport"'), /needs teleport, which is no feature/);
  assert.throws(() => toolOf('x', 'pub struct X;', features), /implements Tool for no type/);
});

test('a tool is in a build only while every feature it needs is', () => {
  const list = wiredTools([tool('whoami', ''), tool('review', '"timecard.meal_breaks", "dvic"')]);
  assert.match(list, /^mod whoami;$/m);
  assert.match(list, /^#\[cfg\(all\(feature = "dvic", feature = "timecard"\)\)\]\nmod review;$/m);
  assert.match(
    list,
    /^ {4}#\[cfg\(all\(feature = "dvic", feature = "timecard"\)\)\]\n {4}&review::T,$/m,
  );
  // With no feature to keep out, the list is one line, as rustfmt keeps it.
  assert.match(
    wiredTools([tool('whoami', '')]),
    /^pub const TOOLS: &\[&dyn AnyTool\] = &\[&whoami::T\];$/m,
  );
});

test("a tool reading a feature's contract brings the contract always, beside the feature", () => {
  const root = workspace();
  try {
    fs.mkdirSync(path.join(root, 'features/dvic/contract'), { recursive: true });
    fs.writeFileSync(
      path.join(root, 'features/dvic/contract/Cargo.toml'),
      '[package]\nname = "dispatch-dvic-contract"\n',
    );
    const reading = toolOf(
      'inspections',
      `use dispatch_dvic_contract::{self as dvic, Asked};\npub struct T;\nimpl Tool for T {\n    const FEATURES: &'static [&'static str] = &["dvic"];\n}\n`,
      features,
    );
    assert.deepEqual(reading.reads, ['dvic']);
    const manifest = ['[features]', mcpMarker, '', '[dependencies]', mcpMarker, ''].join('\n');
    assert.match(
      wiredMcpManifest(root, manifest, features, [reading]),
      /^dispatch-dvic-contract = \{ path = "\.\.\/features\/dvic\/contract" \}$/m,
    );
    assert.throws(
      () => toolOf('x', 'use dispatch_teleport_contract::Beam;\nimpl Tool for X {}', features),
      /reads dispatch_teleport_contract, and teleport is no feature/,
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("the MCP brings each needed feature's crate, and what its tests install, only with it", () => {
  const root = workspace();
  try {
    const manifest = [
      '[features]',
      mcpMarker,
      'default = []',
      '',
      'testing = []',
      '',
      '[dependencies]',
      'dispatch-core = { path = "../core" }',
      mcpMarker,
      '',
    ].join('\n');
    const list = [tool('review', '"timecard.meal_breaks"'), tool('inspections', '"dvic"')];
    const wired = wiredMcpManifest(root, manifest, features, list);
    assert.equal(
      wired,
      [
        '[features]',
        mcpMarker,
        'default = [',
        '    "dvic",',
        '    "timecard",',
        ']',
        'dvic = ["dep:dispatch-cortex", "dep:dispatch-dvic"]',
        'timecard = ["dep:dispatch-cortex", "dep:dispatch-driver-match", "dep:dispatch-paycom", "dep:dispatch-timecard"]',
        '',
        'testing = []',
        '',
        '[dependencies]',
        'dispatch-core = { path = "../core" }',
        mcpMarker,
        'dispatch-cortex = { path = "../collectors/cortex", optional = true }',
        'dispatch-driver-match = { path = "../features/driver_match", optional = true }',
        'dispatch-dvic = { path = "../features/dvic", optional = true }',
        'dispatch-paycom = { path = "../collectors/paycom", optional = true }',
        'dispatch-timecard = { path = "../features/timecard", optional = true }',
        '',
      ].join('\n'),
    );
    // Wired again, it reads the same.
    assert.equal(wiredMcpManifest(root, wired, features, list), wired);

    // The tests install each with what it uses and keeps, while one that brings it is on.
    const product = wiredProduct(root, features, list);
    assert.match(
      product,
      / {4}#\[cfg\(any\(feature = "dvic", feature = "timecard"\)\)\]\n {4}&dispatch_cortex::COLLECTOR,/,
    );
    assert.match(
      product,
      / {4}#\[cfg\(feature = "timecard"\)\]\n {4}&dispatch_driver_match::FEATURE,/,
    );
    assert.match(product, / {4}#\[cfg\(feature = "dvic"\)\]\n {4}&dispatch_dvic::FEATURE,/);
    assert.match(
      wiredProduct(root, features, []),
      /^pub const COLLECTORS: &\[&dyn Collector\] = &\[\];\npub const FEATURES: &\[&Feature\] = &\[\];\n$/m,
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("the app turns on the MCP's Cargo feature of each feature its tools need", () => {
  const marker =
    '# Every feature: written from features/ by `npm run contracts:generate`, to the blank line.';
  const manifest = [
    '[features]',
    marker,
    '',
    '[dependencies]',
    marker,
    '',
    '[dev-dependencies]',
    marker,
    '',
  ].join('\n');
  const wired = wiredManifest(manifest, features, ['dvic']);
  assert.match(wired, /^dvic = \["dep:dispatch-dvic", "dispatch-mcp\/dvic"\]$/m);
  assert.match(wired, /^timecard = \["dep:dispatch-timecard", "driver_match"\]$/m);
});

test("the repository's tools are listed as it holds them", () => {
  const root = path.resolve(import.meta.dirname, '../..');
  const list = tools(root, crates(root));
  const names = list.map((each) => each.module);
  assert(names.includes('whoami') && names.includes('get_profile'), names.join(', '));
  assert.deepEqual(list.find((each) => each.module === 'dvic_short_inspections')?.needs, ['dvic']);
});
