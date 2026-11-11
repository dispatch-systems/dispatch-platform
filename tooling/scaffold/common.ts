import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
/** The repository the scaffold belongs to; `--root` points the generators at another. */
export const repositoryRoot = path.resolve(here, '../..');
const templates = path.join(here, 'templates');

export class UsageError extends Error {}

// ---- Arguments

export type Arguments = {
  positional: string[];
  flags: Set<string>;
  options: Map<string, string>;
};
/** `--flag`, `--option value` or `--option=value`, and the rest in order. */
export function parseArguments(argv: string[], flags: string[], options: string[]): Arguments {
  const parsed: Arguments = { positional: [], flags: new Set(), options: new Map() };
  for (let index = 0; index < argv.length; index++) {
    const argument = argv[index]!;
    if (!argument.startsWith('--')) {
      parsed.positional.push(argument);
      continue;
    }
    const [key, inline] = argument.slice(2).split(/=(.*)/s, 2) as [string, string | undefined];
    if (flags.includes(key) && inline === undefined) parsed.flags.add(key);
    else if (options.includes(key)) {
      const value = inline ?? argv[++index];
      if (!value || value.startsWith('--')) throw new UsageError(`--${key} needs a value`);
      parsed.options.set(key, value);
    } else throw new UsageError(`Unknown option ${argument}`);
  }
  return parsed;
}

// ---- Names

export type Names = {
  /** The directory's name, snake_case: `driver_match`. */
  name: string;
  /** In addresses and file names: `driver-match`. */
  slug: string;
  pascal: string;
  camel: string;
  constant: string;
  label: string;
  /** The crate: `dispatch-driver-match`, used as `dispatch_driver_match`. */
  crate: string;
  ident: string;
};
const NAME = /^[a-z][a-z0-9]*(_[a-z0-9]+)*$/;
// Names the structure rules retire, and the top level's own.
const RESERVED = [
  'agents',
  'app',
  'auth',
  'browsers',
  'collectors',
  'custom',
  'core',
  'drivers',
  'features',
  'mcp',
  'meals',
  'routedata',
  'workforce',
];
export function names(name: string, label?: string): Names {
  if (!NAME.test(name))
    throw new UsageError(`${name} is not a name: use lowercase words joined by _, as driver_match`);
  if (RESERVED.includes(name)) throw new UsageError(`${name} is a retired or reserved name`);
  const words = name.split('_');
  const capital = (word: string) => word[0]!.toUpperCase() + word.slice(1);
  return {
    name,
    slug: words.join('-'),
    pascal: words.map(capital).join(''),
    camel: words[0]! + words.slice(1).map(capital).join(''),
    constant: name.toUpperCase(),
    label: label ?? words.map(capital).join(' '),
    crate: `dispatch-${words.join('-')}`,
    ident: `dispatch_${name}`,
  };
}

// ---- Templates

export type Values = Record<string, string | number | boolean>;
const STANDALONE = /^[ \t]*(\{\{[#^/]\w+\}\})[ \t]*\r?\n/gm;
const TAG = /\{\{([#^/]?)(\w+)\}\}/g;
/**
 * Fills a template: `{{value}}`, `{{#flag}}…{{/flag}}` while `flag` is set, `{{^flag}}…{{/flag}}`
 * while it is not. A line holding only a section tag leaves no line behind. Every name must have
 * a value, so a misspelt one fails here rather than in the output.
 */
export function render(source: string, values: Values, file = 'template'): string {
  const text = source.replace(STANDALONE, '$1');
  const value = (name: string) => {
    if (!(name in values)) throw new Error(`${file}: no value for {{${name}}}`);
    return values[name]!;
  };
  const open: { name: string; shown: boolean }[] = [];
  const shown = () => open.every((section) => section.shown);
  let output = '';
  let index = 0;
  for (const match of text.matchAll(TAG)) {
    const [tag, sigil, name] = match as unknown as [string, string, string];
    if (shown()) output += text.slice(index, match.index);
    index = match.index + tag.length;
    if (sigil === '#' || sigil === '^') {
      const set = Boolean(value(name));
      open.push({ name, shown: sigil === '#' ? set : !set });
    } else if (sigil === '/') {
      const section = open.pop();
      if (section?.name !== name)
        throw new Error(`${file}: {{/${name}}} closes ${section ? section.name : 'nothing'}`);
    } else {
      const filled = value(name);
      if (shown()) output += String(filled);
    }
  }
  if (open.length) throw new Error(`${file}: {{#${open.at(-1)!.name}}} is never closed`);
  return output + text.slice(index);
}
/** `templates/<name>.tmpl`, filled. */
export function template(name: string, values: Values) {
  return render(fs.readFileSync(path.join(templates, `${name}.tmpl`), 'utf8'), values, name);
}

/** Whether lucide-react, which draws the dashboard's icons, has an icon of this name. */
export function isIcon(name: string) {
  const icons = createRequire(import.meta.url)('lucide-react') as Record<string, unknown>;
  return /^[A-Z][A-Za-z0-9]*$/.test(name) && !name.endsWith('Icon') && name in icons;
}
