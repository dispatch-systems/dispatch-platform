import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import test from 'node:test';

// Communications' stylesheets come in whichever order its pages first load them, Settings'
// Message display or Communications itself, and a build bundles them in an order of its own; so
// no two set a property on the same selector, where the one read last would win.

const frontend = new URL('../../frontend/', import.meta.url);

/** The selectors a rule's prelude lists, each of an `:is()`'s read as a selector of its own. */
function selectors(prelude: string): string[] {
  const listed = [''];
  let depth = 0;
  for (const char of prelude) {
    if (char === ',' && depth === 0) listed.push('');
    else listed[listed.length - 1] += char;
    if (char === '(') depth += 1;
    if (char === ')') depth -= 1;
  }
  return listed.flatMap((selector) => {
    const is = /:is\(([^()]*)\)/.exec(selector);
    if (!is) return [selector.replace(/\s+/g, ' ').trim()];
    return selectors(is[1]!).flatMap((each) => selectors(selector.replace(is[0], each)));
  });
}

/** Each property a stylesheet sets on each selector, as `selector { property }`, whatever
 * condition it's under; a keyframe's steps are its animation's own. */
function settings(css: string): Set<string> {
  const rules = css.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^{}]*)\}/g);
  return new Set(
    [...rules].flatMap(([, prelude, body]) => {
      const properties = body!.split(';').flatMap((each) => each.split(':')[0]!.trim() || []);
      return selectors(prelude!)
        .filter((selector) => !/^(from|to|[\d.]+%)$/.test(selector))
        .flatMap((selector) => properties.map((property) => `${selector} { ${property} }`));
    }),
  );
}

test('no two of its stylesheets set a property on the same selector, so none hangs on which loads first', () => {
  const setIn = new Map<string, string>();
  const twice: string[] = [];
  for (const file of readdirSync(frontend, { recursive: true, encoding: 'utf8' })) {
    if (!file.endsWith('.css')) continue;
    for (const each of settings(readFileSync(new URL(file, frontend), 'utf8'))) {
      const first = setIn.get(each);
      if (first) twice.push(`${each}: ${first} and ${file}`);
      else setIn.set(each, file);
    }
  }
  assert.deepEqual(twice, []);
});
