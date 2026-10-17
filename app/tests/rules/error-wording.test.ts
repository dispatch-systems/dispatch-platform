import test from 'node:test';
import { holds } from './support/holds.js';
import { productRust } from './support/manifests.js';
import { features, files, isFile, read } from './support/repo.js';
import { lexFile } from './support/rust.js';

// Every error a feature refuses a member with is worded somewhere its screens load: in its API
// client (`wordedApi({ code: 'Words.' })`), or in its manifest's `errors` for a code something
// outside its own screens raises. A code without wording reaches the member as the code itself.
// Core answers for the codes it raises itself or words for every owner, such as `invalid_input`.

/**
 * The codes core answers for: those it words for every owner in its API client, and those it
 * raises itself, which a feature raises as core does.
 */
function coresCodes(): Set<string> {
  const { masked, literals } = lexFile('core/shell/frontend/runtime/api.ts');
  const open = masked.indexOf('const labels');
  const close = masked.indexOf('};', open);
  const worded = literals
    .filter(({ start }) => start > open && start < close)
    .map(({ value }) => value);
  return new Set([...worded, ...refused('core', 'core/')]);
}

/**
 * The codes an owner's product Rust under `under` refuses a member with: `Error::new` and
 * `ensure` at 4xx. For a feature, its routes: what its API refuses.
 */
function refused(dir: string, under = `${dir}/api/`): Set<string> {
  const codes = new Set<string>();
  for (const file of files.filter((file) => file.startsWith(under) && productRust(file))) {
    const { masked, literals } = lexFile(file);
    for (const match of masked.matchAll(/\b(Error::new|ensure)\s*\(/g)) {
      const open = match.index + match[0].length - 1;
      // The arguments up to the closing parenthesis: the code is the string before the status.
      let depth = 0;
      let close = open;
      for (; close < masked.length; close++) {
        if ('([{'.includes(masked[close]!)) depth++;
        else if (')]}'.includes(masked[close]!) && --depth === 0) break;
      }
      const inside = literals.filter(({ start, end }) => start > open && end <= close);
      const status = /,\s*(\d{3})\s*,?\s*$/.exec(masked.slice(open + 1, close));
      const code = inside.at(-1)?.value;
      if (code && status && status[1]!.startsWith('4')) codes.add(code);
    }
  }
  return codes;
}

/** The codes a feature words: its API client's `wordedApi` and its manifest's `errors`. */
function worded(dir: string): Set<string> {
  const codes = new Set<string>();
  for (const file of [`${dir}/api/client.ts`, `${dir}/frontend/feature.ts`]) {
    if (!isFile(file)) continue;
    for (const match of read(file).matchAll(/^\s+([a-z_]+):\s*['"`\n]/gm)) codes.add(match[1]!);
  }
  return codes;
}

test('every error a feature refuses a member with is worded', () => {
  const core = coresCodes();
  holds(
    'error-wording',
    'codes',
    features().flatMap(({ dir }) => {
      const words = worded(dir);
      return [...refused(dir)]
        .filter((code) => !core.has(code) && !words.has(code))
        .map((code) => `${dir} refuses with ${code}, which nothing words`);
    }),
  );
});
