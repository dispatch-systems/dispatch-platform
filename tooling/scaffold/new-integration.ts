import path from 'node:path';
import { installing, mergeRust, mergeToml, mergeTypeScript, rustfmt } from './add.js';
import {
  UsageError,
  change,
  emptyPlan,
  exists,
  features,
  files,
  format,
  formatRust,
  isIcon,
  names as namesOf,
  parseArguments,
  read,
  run,
  template,
  type Generator,
  type Plan,
} from './scaffold.js';

// `dispatchdev new integration <name> [options]`: an integration through Slack, in a folder of
// its own, `integrations/<name>/`, the same shape whoever owns it. One of Slack's own (the
// default) has all of it there: its declaration, its endpoint and the type it answers with, its
// card and its section of a workspace's settings, their calls and its audit wording; and it is
// listed in Slack's manifests, Slack's list of its own, and its route list. One of another
// feature's (`--to <feature>`) is its declaration and its card, listed in that feature's
// manifests; what it does in Slack it does through `dispatch_slack::kit`. What's left to write
// is marked TODO(scaffold).

export const usage = `Usage: dispatchdev new integration <name> [options]

  --to <feature>      the feature it belongs to (default: slack, one of Slack's own)
  --label <label>     its name as people read it (default: from <name>)
  --icon <name>       its icon, from lucide-react (default: Blocks)
  --needs <scopes>    the Slack permissions it needs, as files:read,chat:write
  --hears <events>    the Slack events it's told of, as file_shared, each handled in heard
  --dry-run           print what it would write and change nothing
  --json              print what it made, or would make, as JSON
  --out <dir>         with --dry-run, write the files under <dir> instead
  --root <dir>        the repository to write into (default: this one)`;

const FLAGS = ['dry-run'];
const OPTIONS = ['to', 'label', 'icon', 'needs', 'hears', 'out', 'root'];
const SLACK = 'features/slack';

/** The quoted items of a comma-separated option, each as Slack spells such a name. */
function listed(value: string | undefined, what: string) {
  const items = (value ?? '').split(',').filter(Boolean);
  for (const item of items)
    if (!/^[a-z][a-z0-9_.:]*$/.test(item))
      throw new UsageError(
        `--${what} names Slack's own, as files:read or file_shared: ${item} is none`,
      );
  return items.map((item) => `"${item}"`).join(', ');
}

export async function planIntegration(root: string, argv: string[]) {
  const args = parseArguments(argv, FLAGS, OPTIONS);
  if (args.positional.length !== 1) throw new UsageError('Name one integration');
  const names = namesOf(args.positional[0]!, args.options.get('label'));
  const owner = args.options.get('to') ?? 'slack';
  if (!features(root).includes(owner))
    throw new UsageError(`--to names a feature, and ${owner} is none`);
  if (!features(root).includes('slack'))
    throw new UsageError(
      'An integration through Slack needs Slack: this build has no features/slack',
    );
  const own = owner === 'slack';
  const dir = `features/${owner}/integrations/${names.name}`;
  if (exists(root, dir)) throw new UsageError(`${dir} is there already`);
  const icon = args.options.get('icon') ?? 'Blocks';
  if (args.options.has('icon') && !isIcon(icon))
    throw new UsageError(`--icon names a lucide-react icon, as Truck: ${icon} is none`);
  const hears = listed(args.options.get('hears'), 'hears');
  const ownerManifest = read(root, `features/${owner}/feature.rs`);
  const ownerLabel = /\b(?:optional|mandatory)\(\s*"[^"]+",\s*"([^"]+)"/.exec(ownerManifest)?.[1];
  // Its card and section follow those listed already: Slack's own sections in its manifest,
  // and each integration's card and section in its folder.
  const declared = [
    read(root, `${SLACK}/frontend/feature.ts`),
    ...files(root, `${SLACK}/integrations`, /frontend\/integration\.ts$/).map((file) =>
      read(root, file),
    ),
  ].join('\n');
  const after = (pattern: RegExp) =>
    (Math.floor(
      Math.max(0, ...[...declared.matchAll(pattern)].map((match) => Number(match[1]))) / 10,
    ) +
      1) *
    10;
  const cards = after(/: ConnectionIntegration = \{[\s\S]*?\border: (\d+),/g);
  const sections = Math.max(
    after(/: ConnectionSection = \{[\s\S]*?\border: (\d+),/g),
    after(/connectionSections: \[[\s\S]*?\border: (\d+),/g),
  );
  const values = {
    ...names,
    own,
    owner,
    ownerLabel: ownerLabel ?? owner,
    icon,
    needs: listed(args.options.get('needs'), 'needs'),
    hears,
    heard: Boolean(hears),
    order: cards,
    sectionOrder: sections,
  };

  const plan: Plan = emptyPlan();
  const write = async (source: string, target: string) => {
    const file = `${dir}/${target}`;
    const toRoot = '../'.repeat(path.posix.dirname(file).split('/').length);
    plan.files.set(
      file,
      await format(file, template(`integration/${source}`, { ...values, toRoot })),
    );
  };
  await write('mod.rs', 'mod.rs');
  await write('frontend/integration.ts', 'frontend/integration.ts');
  if (own) {
    await write('api.rs', 'api.rs');
    await write('types.rs', 'types.rs');
    await write('frontend/Section.tsx', `frontend/${names.pascal}Section.tsx`);
    await write('frontend/client.ts', 'frontend/client.ts');
    await write('frontend/audit-wording.ts', 'frontend/audit-wording.ts');
  }
  formatRust(plan);

  const edit = (file: string, apply: (text: string) => string) => change(plan, root, file, apply);
  const left: string[] = [];
  // Its owner's list of its integrations, a module each.
  const list = `features/${owner}/integrations/mod.rs`;
  if (exists(root, list))
    await edit(list, (text) => {
      const modules = [...text.matchAll(/^pub\(crate\) mod \w+;$/gm)];
      const last = modules.at(-1)!;
      const at = last.index! + last[0].length;
      let next = `${text.slice(0, at)}\npub(crate) mod ${names.name};${text.slice(at)}`;
      // Its line in the list of folders, which says what it does.
      const bullets = [...next.matchAll(/^\/\/! - `\w+`: .*$/gm)];
      const bullet = bullets.at(-1);
      if (bullet)
        next = `${next.slice(0, bullet.index! + bullet[0].length)}\n//! - \`${names.name}\`: ${names.label}, TODO(scaffold): what it does.${next.slice(bullet.index! + bullet[0].length)}`;
      if (own)
        next = next.replace(
          /pub\(crate\) const ALL: &\[&Own\] = &\[([^\]]*)\];/,
          (_, items: string) =>
            `pub(crate) const ALL: &[&Own] = &[${items.trim().replace(/,$/, '')}, &${names.name}::OWN];`,
        );
      return next;
    });
  else
    plan.files.set(
      list,
      `//! ${ownerLabel ?? owner}'s integrations through other features' connections, a folder each, in\n//! the shape \`dispatchdev new integration\` writes.\npub(crate) mod ${names.name};\n`,
    );
  // Its declaration in its owner's manifest; one of another feature's is its first module there,
  // and works through Slack, which its feature then depends on, as the registry holds it to.
  const piece = `mod integrations;\n\nuse dispatch_core::manifest::{Feature, feature};\n\npub const FEATURE: Feature = Feature {\n${own ? '' : '    depends_on: &["slack"],\n'}    integrations: &[integrations::${names.name}::INTEGRATION],\n    ..feature("${owner}")\n};\n`;
  // Its feature's tests install it with Slack, as they install it with what else it uses.
  if (!own) installing(root, plan, `features/${owner}`, { feature: 'slack' });
  if (!own)
    await edit(`features/${owner}/Cargo.toml`, (text) =>
      mergeToml(text, '[dependencies]\ndispatch-slack = { path = "../slack" }\n'),
    );
  await edit(`features/${owner}/feature.rs`, (text) =>
    mergeRust(text, piece, left, [], () => true),
  );
  // Its card, and one of Slack's own its section, in its owner's frontend manifest.
  const frontend = `features/${owner}/frontend/feature.ts`;
  if (!exists(root, frontend))
    left.push(`${frontend} is where its card goes: connectionIntegrations: [${names.camel}]`);
  else
    await edit(frontend, (text) =>
      mergeTypeScript(
        text,
        `import { ${names.camel}${own ? `, ${names.camel}Section` : ''} } from '../integrations/${names.name}/frontend/integration.js';\n\nexport const feature: FrontendFeature = {\n  name: '${owner}',\n  connectionIntegrations: [${names.camel}],\n${own ? `  connectionSections: [${names.camel}Section],\n` : ''}};\n`,
        names.pascal,
      ),
    );
  if (own) {
    // Its type, written to TypeScript with Slack's own and re-exported where Slack's are.
    await edit(
      `${SLACK}/api/types.rs`,
      (text) => `${text.trimEnd()}\npub use crate::integrations::${names.name}::types::*;\n`,
    );
    await edit(`${SLACK}/feature.rs`, (text) =>
      text
        .replace(
          /pub use api::types::\{([^}]*)\};/,
          (_, list: string) =>
            `pub use api::types::{${list.trim().replace(/,$/, '')}, Slack${names.pascal}};`,
        )
        .replace(
          /dispatch_core::typescript!\(\s*cfg,([^)]*)\)/,
          (_, list: string) =>
            `dispatch_core::typescript!(cfg, ${list.trim().replace(/,$/, '')}, Slack${names.pascal},)`,
        ),
    );
    // Its audit wording, among Slack's.
    await edit(`${SLACK}/frontend/audit-wording.ts`, (text) =>
      text
        .replace(
          /(import \{ phrases as \w+ \} from '[^']+';\n)(?!import \{ phrases)/,
          `$1import { phrases as ${names.camel} } from '../integrations/${names.name}/frontend/audit-wording.js';\n`,
        )
        .replace(
          /phrases: \{ \.\.\.phrases,([^}]*)\}/,
          (_, list: string) => `phrases: { ...phrases,${list.trimEnd()}, ...${names.camel} }`,
        )
        .replace(
          /(\n\s+\.\.\.Object\.keys\(\w+\),)(?![\s\S]*\.\.\.Object\.keys)/,
          `$1\n    ...Object.keys(${names.camel}),`,
        ),
    );
    // Its line among Slack's own in Slack's README.
    await edit(`${SLACK}/README.md`, (text) => {
      const own = text.indexOf('## Its own integrations');
      const next = text.indexOf('\n## ', own + 1);
      const section = text.slice(own, next < 0 ? text.length : next);
      const last = [...section.matchAll(/^- \*\*[^*]+\*\*[^\n]*(?:\n {2,}\S[^\n]*)*$/gm)].at(-1);
      if (!last) return text;
      const at = own + last.index! + last[0].length;
      return `${text.slice(0, at)}\n- **${names.label},** Slack's own integration (\`slack.${names.name}\`, \`integrations/${names.name}/\`): TODO(scaffold): what it does, what it keeps and when it runs.${text.slice(at)}`;
    });
    // Its route, in Slack's list of every route it registers.
    await edit(`${SLACK}/tests/api/routes.txt`, (text) => {
      const [comments, ...routes] = text.split(/\n(?=[A-Z]+ \/)/);
      const added = `GET /api/dsp/slack/workspaces/{team}/${names.slug} Dsp("slack.manage") Read`;
      return `${[comments, ...[...routes.map((line) => line.trim()), added].sort()].join('\n')}\n`;
    });
  }
  for (const [file, content] of plan.changes)
    if (file.endsWith('.rs')) plan.changes.set(file, rustfmt(content));
  plan.notes.push(...left.map((note) => `By hand: ${note}.`));
  if (!own) plan.notes.push(`What it does in Slack it does through dispatch_slack::kit, as Bot.`);
  plan.notes.push(
    "`dispatchdev generate` writes what follows from it, and Slack's app manifest snapshot names what it asks Slack for.",
  );
  return { plan, args };
}

export const generator: Generator = {
  kind: 'integration',
  about: "an integration through Slack, in a folder of its own: Slack's, or another feature's",
  usage,
  flags: FLAGS,
  options: OPTIONS,
  plan: planIntegration,
  generates: true,
};

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename))
  await run(generator);
