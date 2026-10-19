import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// The first step of `npm run contracts:generate`: lists every feature
// in features/ in the app, as only the app may. Its crate goes in the app's Cargo manifest, as
// a Cargo feature the product has by default and a build may leave out, and its manifest in
// the registry's list, `app/backend/features.rs`. So adding a feature is adding its folder, and
// removing one is removing it. The rules check holds the app to what this writes.
//
// It lists the MCP's tools the same way, from the files in mcp/tools/: `mcp/tools/mod.rs`, each
// tool while every feature it needs is in the build. A feature a tool needs is a Cargo feature
// of the MCP's too, which brings the feature's crate for the tool to use, and which the app's
// Cargo feature of that name turns on; the MCP's tests install it, with what it keeps and uses
// (`mcp/tests/support/product.rs`). So adding a tool is adding its file.

export const appManifest = 'app/backend/Cargo.toml';
export const featureList = 'app/backend/features.rs';
/** Each of the app manifest's sections that lists the features starts at this line. */
export const marker =
  '# Every feature: written from features/ by `npm run contracts:generate`, to the blank line.';

/** What the app needs of a feature's crate. */
export type Crate = {
  /** Its folder's name, which is the app's Cargo feature for it. */
  name: string;
  /** Its crate: `dispatch-driver-match`. */
  crate: string;
  /** Whether it writes its API types to TypeScript, through a `ts` feature. */
  ts: boolean;
  /** The other features whose crates it uses. */
  uses: string[];
  /** The collectors whose crates it uses, by folder name. */
  collectors: string[];
};

const read = (root: string, file: string) => fs.readFileSync(path.join(root, file), 'utf8');
/** A Cargo manifest's `[section]`, without its header. */
function section(manifest: string, name: string) {
  const lines = manifest.split('\n');
  const start = lines.findIndex((line) => line.trim() === `[${name}]`);
  if (start < 0) return '';
  const end = lines.findIndex((line, index) => index > start && /^\[/.test(line));
  return lines.slice(start + 1, end < 0 ? undefined : end).join('\n');
}

/** Every feature in `root`'s features/, by name: a folder with a manifest and a crate. */
export function crates(root: string): Crate[] {
  const dir = path.join(root, 'features');
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) =>
      ['feature.rs', 'Cargo.toml'].every((file) => fs.existsSync(path.join(dir, name, file))),
    )
    .sort()
    .map((name) => crateOf(name, read(root, `features/${name}/Cargo.toml`)));
}
/** What the app needs of the feature `name`, from its Cargo manifest. */
export function crateOf(name: string, manifest: string): Crate {
  const crate = /^name\s*=\s*"([^"]+)"/m.exec(section(manifest, 'package'))?.[1];
  if (!crate) throw new Error(`features/${name}/Cargo.toml names no package`);
  const dependencies = section(manifest, 'dependencies');
  const uses = [...dependencies.matchAll(/path\s*=\s*"\.\.\/([a-z0-9_]+)"/g)]
    .map((found) => found[1]!)
    .sort();
  const collectors = [
    ...dependencies.matchAll(/path\s*=\s*"\.\.\/\.\.\/collectors\/([a-z0-9_]+)"/g),
  ]
    .map((found) => found[1]!)
    .sort();
  return { name, crate, ts: /^ts\s*=/m.test(section(manifest, 'features')), uses, collectors };
}

/**
 * The app manifest with each section's feature list as `features` has it. A feature the MCP's
 * tools need, of `mcp`, turns on the MCP's Cargo feature of its name too.
 */
export function wiredManifest(manifest: string, features: Crate[], mcp: string[] = []) {
  const lists: Record<string, string[]> = {
    features: [
      'default = [',
      ...features.map(({ name }) => `    "${name}",`),
      ']',
      ...features.map(({ name, crate, uses }) => {
        const entries = [
          `dep:${crate}`,
          ...uses,
          ...(mcp.includes(name) ? [`dispatch-mcp/${name}`] : []),
        ];
        return `${name} = [${entries.map((entry) => `"${entry}"`).join(', ')}]`;
      }),
    ],
    dependencies: features.map(
      ({ name, crate }) => `${crate} = { path = "../../features/${name}", optional = true }`,
    ),
    'dev-dependencies': features
      .filter(({ ts }) => ts)
      .map(
        ({ name, crate }) => `${crate} = { path = "../../features/${name}", features = ["ts"] }`,
      ),
  };
  return filled(manifest, appManifest, marker, lists);
}

/**
 * `manifest`, the Cargo manifest `file`, with the lines after each `marker` line, to the blank
 * line, as its section's entry of `lists` has them.
 */
function filled(manifest: string, file: string, marker: string, lists: Record<string, string[]>) {
  const lines = manifest.split('\n');
  const out: string[] = [];
  let current = '';
  const left = { ...lists };
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index]!;
    current = /^\[([^\]]+)\]$/.exec(line)?.[1] ?? current;
    out.push(line);
    if (line !== marker) continue;
    const list = left[current];
    if (!list) throw new Error(`${file}: [${current}] lists no features`);
    out.push(...list);
    delete left[current];
    while (index + 1 < lines.length && lines[index + 1]!.trim()) index++;
  }
  const missing = Object.keys(left);
  if (missing.length)
    throw new Error(`${file}: [${missing.join('], [')}] has no line reading ${marker}`);
  return out.join('\n');
}

/** The registry's list of features, each while the app's Cargo feature of its name is on. */
export function wiredList(features: Crate[]) {
  const ident = (crate: string) => crate.replaceAll('-', '_');
  const entries = features.map(
    ({ name, crate }) => `    #[cfg(feature = "${name}")]\n    &${ident(crate)}::FEATURE,\n`,
  );
  // Each feature that writes its API types to TypeScript lists them itself.
  const typescript = features
    .filter(({ ts }) => ts)
    .map(
      ({ name, crate }) =>
        `    #[cfg(feature = "${name}")]\n    all.extend(${ident(crate)}::typescript(cfg));\n`,
    );
  return (
    "//! Every feature in features/, written by `npm run contracts:generate`: the registry's,\n" +
    "//! each while the app's Cargo feature of its name is on. The registry puts them in their\n" +
    '//! places.\n' +
    'use dispatch_core::manifest::Feature;\n\n' +
    `pub const FEATURES: &[&Feature] = &[\n${entries.join('')}];\n\n` +
    "/// Every feature's API types in TypeScript, as each lists them, for the app's export test.\n" +
    '#[cfg(test)]\n' +
    '// A build with no feature has none to ask, and uses neither.\n' +
    '#[allow(unused_variables)]\n' +
    'pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {\n' +
    '    #[allow(unused_mut)]\n' +
    '    let mut all = dispatch_core::Typescript::new();\n' +
    typescript.join('') +
    '    all\n' +
    '}\n'
  );
}

export const mcpManifest = 'mcp/Cargo.toml';
export const toolList = 'mcp/tools/mod.rs';
export const productList = 'mcp/tests/support/product.rs';
/** Each of the MCP manifest's sections that lists the features its tools need starts here. */
export const mcpMarker =
  "# Every feature the MCP's tools need: written from mcp/tools/ by `npm run contracts:generate`, to the blank line.";

/** A tool, as its file in mcp/tools/ declares it. */
export type Tool = {
  /** Its file's name, which is its module's. */
  module: string;
  /** The type implementing `Tool`. */
  type: string;
  /** The features it needs, by folder name, without their parts. */
  needs: string[];
  /** The features whose contracts it reads through (`dispatch_<feature>_contract`), by folder name. */
  reads: string[];
};

/** Every tool in `root`'s mcp/tools/, by file name. */
export function tools(root: string, features: Crate[]): Tool[] {
  const dir = path.join(root, 'mcp/tools');
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((file) => file.endsWith('.rs') && file !== 'mod.rs')
    .sort()
    .map((file) => toolOf(file.slice(0, -3), read(root, `mcp/tools/${file}`), features));
}
/** Rust source without its comments, strings and characters, each matched from where it starts. */
const code = (source: string) =>
  source.replace(/\/\/[^\n]*|\/\*[\s\S]*?\*\/|"(?:[^"\\]|\\.)*"|'(?:[^'\\\n]|\\.)'/g, '');
/** What the wiring needs of the tool in the file `module`.rs. */
export function toolOf(module: string, source: string, features: Crate[]): Tool {
  const type = /\bimpl\s+Tool\s+for\s+(\w+)/.exec(source)?.[1];
  if (!type) throw new Error(`mcp/tools/${module}.rs implements Tool for no type`);
  const declared = /const\s+FEATURES\s*:[^=]*=\s*&\s*\[([^\]]*)\]/.exec(source)?.[1] ?? '';
  const needs = [
    ...new Set([...declared.matchAll(/"([^"]+)"/g)].map((found) => found[1]!.split('.')[0]!)),
  ].sort();
  for (const name of needs)
    if (!features.some((feature) => feature.name === name))
      throw new Error(`mcp/tools/${module}.rs needs ${name}, which is no feature in features/`);
  const reads = [
    ...new Set(
      [...code(source).matchAll(/\bdispatch_(\w+)_contract\b/g)].map((found) => found[1]!),
    ),
  ].sort();
  for (const name of reads)
    if (!features.some((feature) => feature.name === name))
      throw new Error(
        `mcp/tools/${module}.rs reads dispatch_${name}_contract, and ${name} is no feature in features/`,
      );
  return { module, type, needs, reads };
}

/** The features the MCP's tools need, by name. */
export const needed = (list: Tool[]) => [...new Set(list.flatMap((tool) => tool.needs))].sort();
/**
 * What a test installs with each feature the tools need, by the MCP's Cargo feature that
 * brings it: the feature, the features it uses, however indirectly, and their collectors.
 */
function installs(features: Crate[], list: Tool[]) {
  const by = (name: string) => features.find((feature) => feature.name === name)!;
  const brought = {
    features: new Map<string, Set<string>>(),
    collectors: new Map<string, Set<string>>(),
  };
  for (const need of needed(list)) {
    const reached = new Set<string>();
    const visit = (name: string) => {
      if (reached.has(name)) return;
      reached.add(name);
      by(name).uses.forEach(visit);
    };
    visit(need);
    for (const name of reached) {
      brought.features.set(name, (brought.features.get(name) ?? new Set()).add(need));
      for (const collector of by(name).collectors)
        brought.collectors.set(
          collector,
          (brought.collectors.get(collector) ?? new Set()).add(need),
        );
    }
  }
  return brought;
}
/** A feature's contract crate, from its manifest. */
const contractCrate = (root: string, name: string) => {
  const manifest = `features/${name}/contract/Cargo.toml`;
  if (!fs.existsSync(path.join(root, manifest)))
    throw new Error(`a tool reads ${name}'s contract, and ${manifest} isn't there`);
  const crate = /^name\s*=\s*"([^"]+)"/m.exec(section(read(root, manifest), 'package'))?.[1];
  if (!crate) throw new Error(`${manifest} names no package`);
  return crate;
};
/** A collector's crate, from its manifest. */
const collectorCrate = (root: string, name: string) => {
  const crate = /^name\s*=\s*"([^"]+)"/m.exec(
    section(read(root, `collectors/${name}/Cargo.toml`), 'package'),
  )?.[1];
  if (!crate) throw new Error(`collectors/${name}/Cargo.toml names no package`);
  return crate;
};

/**
 * The MCP's manifest with the features its tools need: each a Cargo feature of its own, on by
 * default for the MCP's own tests, that brings the feature's crate and every crate its tests
 * install with it, each an optional dependency; and every contract its tools read through,
 * always, as a contract stays whatever a build leaves out.
 */
export function wiredMcpManifest(root: string, manifest: string, features: Crate[], list: Tool[]) {
  const brought = installs(features, list);
  const crateOf = (name: string) => features.find((feature) => feature.name === name)!.crate;
  const names = needed(list);
  const dependencies = [
    ...[...brought.features.keys()].map((name) => ({
      crate: crateOf(name),
      line: `${crateOf(name)} = { path = "../features/${name}", optional = true }`,
    })),
    ...[...brought.collectors.keys()].map((name) => ({
      crate: collectorCrate(root, name),
      line: `${collectorCrate(root, name)} = { path = "../collectors/${name}", optional = true }`,
    })),
    ...[...new Set(list.flatMap((tool) => tool.reads))].map((name) => ({
      crate: contractCrate(root, name),
      line: `${contractCrate(root, name)} = { path = "../features/${name}/contract" }`,
    })),
  ].sort((a, b) => a.crate.localeCompare(b.crate));
  const brings = (need: string) =>
    [
      ...[...brought.features].filter(([, by]) => by.has(need)).map(([name]) => crateOf(name)),
      ...[...brought.collectors]
        .filter(([, by]) => by.has(need))
        .map(([name]) => collectorCrate(root, name)),
    ]
      .sort()
      .map((crate) => `"dep:${crate}"`);
  return filled(manifest, mcpManifest, mcpMarker, {
    features: [
      ...(names.length
        ? ['default = [', ...names.map((name) => `    "${name}",`), ']']
        : ['default = []']),
      ...names.map((name) => `${name} = [${brings(name).join(', ')}]`),
    ],
    dependencies: dependencies.map(({ line }) => line),
  });
}

/** `opener`, its items and its close, as rustfmt writes a list: on one line when it fits. */
function rustList(opener: string, items: { cfg?: string; item: string }[], close = '];') {
  const line = `${opener}${items.map(({ item }) => item).join(', ')}${close}`;
  const inner = items.map(({ item }) => item).join(', ');
  if (items.every(({ cfg }) => !cfg) && line.length <= 100 && inner.length + 2 <= 60)
    return `${line}\n`;
  const lines = items.map(({ cfg, item }) => `${cfg ? `    ${cfg}\n` : ''}    ${item},\n`);
  return `${opener}\n${lines.join('')}${close}\n`;
}
/** The attribute that keeps something to builds with every one of `needs`, or none. */
function cfgOf(needs: string[], any = false) {
  if (!needs.length) return undefined;
  if (needs.length === 1) return `#[cfg(feature = "${needs[0]}")]`;
  const each = needs.map((name) => `feature = "${name}"`).join(', ');
  return `#[cfg(${any ? 'any' : 'all'}(${each}))]`;
}

/** The MCP's list of tools, each while every feature it needs is in the build. */
export function wiredTools(list: Tool[]) {
  const modules = list
    .map(({ module, needs }) => `${cfgOf(needs) ? `${cfgOf(needs)}\n` : ''}mod ${module};\n`)
    .join('');
  return (
    '//! Every tool agents use, a file each, written by `npm run contracts:generate` from the\n' +
    "//! files here: a tool that needs features is in a build only while the MCP's Cargo feature of\n" +
    '//! each is on. A tool is a type implementing `toolbox::Tool`; `dispatchdev new tool <name>`\n' +
    '//! makes one that works as written, with its test. The connection, never the tool, says who\n' +
    '//! may use it.\n' +
    modules +
    '\nuse crate::toolbox::AnyTool;\n\n' +
    "/// Every tool, by its file's name.\n" +
    rustList(
      'pub const TOOLS: &[&dyn AnyTool] = &[',
      list.map(({ module, type, needs }) => ({ cfg: cfgOf(needs), item: `&${module}::${type}` })),
    )
  );
}

/** What the MCP's tests install of the product: the features its tools need, and theirs. */
export function wiredProduct(root: string, features: Crate[], list: Tool[]) {
  const brought = installs(features, list);
  const ident = (crate: string) => crate.replaceAll('-', '_');
  const crateOf = (name: string) => features.find((feature) => feature.name === name)!.crate;
  const entries = (by: Map<string, Set<string>>, crate: (name: string) => string, item: string) =>
    [...by]
      .map(([name, needs]) => ({
        cfg: cfgOf([...needs].sort(), true),
        item: `&${ident(crate(name))}::${item}`,
      }))
      .sort((a, b) => a.item.localeCompare(b.item));
  return (
    "//! The product's features the MCP's tools need, with the features those use and the\n" +
    '//! collectors whose collections they keep, written by `npm run contracts:generate`: its tests\n' +
    "//! install them, each while the MCP's Cargo feature of a feature that brings it is on.\n" +
    'use dispatch_core::manifest::{Collector, Feature};\n\n' +
    rustList(
      'pub const COLLECTORS: &[&dyn Collector] = &[',
      entries(brought.collectors, (name) => collectorCrate(root, name), 'COLLECTOR'),
    ) +
    rustList('pub const FEATURES: &[&Feature] = &[', entries(brought.features, crateOf, 'FEATURE'))
  );
}

/** Each file the wiring writes, as it should read for `root`'s features and tools. */
export function wiring(root: string) {
  const features = crates(root);
  const list = tools(root, features);
  const files = new Map([
    [appManifest, wiredManifest(read(root, appManifest), features, needed(list))],
    [featureList, wiredList(features)],
  ]);
  if (fs.existsSync(path.join(root, mcpManifest))) {
    files.set(mcpManifest, wiredMcpManifest(root, read(root, mcpManifest), features, list));
    files.set(toolList, wiredTools(list));
    files.set(productList, wiredProduct(root, features, list));
  }
  return files;
}

/** The files the wiring would change in `root`: each that doesn't read as it should. */
export function unwired(root: string, wired = wiring(root)) {
  return [...wired]
    .filter(([file, text]) => {
      const at = path.join(root, file);
      return !fs.existsSync(at) || fs.readFileSync(at, 'utf8') !== text;
    })
    .map(([file]) => file);
}

/**
 * Writes the wiring into `root`; Cargo.lock then lists the workspace's crates as they now are,
 * without the network, since builds run with `--locked`. Answers the files it changed.
 */
export function wire(root: string) {
  const wired = wiring(root);
  const changed = unwired(root, wired);
  for (const file of changed) fs.writeFileSync(path.join(root, file), wired.get(file)!);
  if (changed.includes(appManifest) || changed.includes(mcpManifest)) {
    const lock = spawnSync('cargo', ['update', '--workspace', '--offline', '--quiet'], {
      cwd: root,
      stdio: 'inherit',
    });
    if (lock.status !== 0) throw new Error('cargo update --workspace --offline failed');
  }
  return changed;
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const changed = wire(root);
  process.stdout.write(
    changed.length ? `Listed the features and tools anew in ${changed.join(', ')}.\n` : '',
  );
}
