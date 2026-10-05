import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

function files(directory: string, extension: RegExp): string[] {
  return fs
    .readdirSync(directory, { recursive: true, encoding: 'utf8' })
    .filter((name) => extension.test(name))
    .map((name) => path.join(directory, name));
}

// Read selector preludes, skipping comments, strings and declaration values.
// This also visits selectors inside @media, @layer and nested CSS rules.
function classes(css: string): Set<string> {
  const names = new Set<string>();
  let prelude = '';
  const parts = css.match(/\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\\.|[^]/g) ?? [];
  for (const part of parts) {
    if (part.startsWith('/*')) continue;
    if (part === '{') {
      if (!prelude.trim().startsWith('@')) {
        for (const match of prelude.matchAll(/\.(-?[A-Za-z_][A-Za-z0-9_-]*)/g))
          names.add(match[1]!);
      }
      prelude = '';
    } else if (part === ';' || part === '}') {
      prelude = '';
    } else {
      prelude += part.startsWith('"') || part.startsWith("'") ? '""' : part;
    }
  }
  return names;
}

// Every owner's frontend/: the app's entry, core's parts, the features and the collectors.
const frontends = [
  'app/frontend',
  ...['core', 'features', 'collectors'].flatMap((top) =>
    fs
      .readdirSync(top)
      .map((name) => path.join(top, name, 'frontend'))
      .filter((directory) => fs.existsSync(directory)),
  ),
];

// Every owner's API types and validators: the files of its api/ besides its client.
const apiTypes = ['core', 'features', 'collectors']
  .flatMap((top) => fs.readdirSync(top).map((name) => path.join(top, name, 'api')))
  .filter((directory) => fs.existsSync(directory))
  .flatMap((directory) => files(directory, /\.tsx?$/))
  .filter((file) => path.basename(file) !== 'client.ts');

const dynamicClasses = new Set([
  // core/shell/frontend/ui/DspAvatar.tsx: tone-${tone}, where tone is a hash modulo five.
  'tone-1',
  'tone-2',
  'tone-3',
  'tone-4',
  // core/accounts/frontend/settings/ThemeSection.tsx: theme-preview-${value}.
  'theme-preview-system',
]);

test('every dashboard CSS class occurs as a source token or is explicitly dynamic', () => {
  const sourceFiles = frontends.flatMap((directory) => files(directory, /\.tsx?$/));
  sourceFiles.push(...apiTypes);
  sourceFiles.push('core/server/backend/mail/templates.rs');
  const tokens = new Set(
    sourceFiles.flatMap((file) => fs.readFileSync(file, 'utf8').match(/[A-Za-z0-9_-]+/g) ?? []),
  );
  // Badge's status values and meal-source's lower-case provider names already occur
  // as tokens in these sources; only constructed names absent from them go above.
  const missing: string[] = [];
  const defined = new Set<string>();
  for (const file of frontends.flatMap((directory) => files(directory, /\.css$/))) {
    for (const name of classes(fs.readFileSync(file, 'utf8'))) {
      defined.add(name);
      if (!tokens.has(name) && !dynamicClasses.has(name)) missing.push(`${file}: .${name}`);
    }
  }
  for (const name of dynamicClasses)
    assert(defined.has(name), `remove stale dynamic allowance: .${name}`);
  assert.deepEqual(missing, [], 'remove unused selectors or document their dynamic construction');
});

test('the CSS scan reads nested selectors without treating declarations or strings as classes', () => {
  assert.deepEqual(
    [
      ...classes(`
    /* .comment */
    @layer base { @media (min-width: 1.5em) {
      .parent:is(.active, .ready)[data-label=".attribute"] {
        background: url("image.decoy"); content: ".content { .fake }";
        opacity: .5;
        & > .child { color: red; }
      }
    }}
    @keyframes pulse { 50% { opacity: .5; } }
  `),
    ].sort(),
    ['active', 'child', 'parent', 'ready'],
  );
});
