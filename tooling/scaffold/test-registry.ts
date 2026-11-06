import { files, read, type Plan } from './scaffold.js';
import { items } from './source-editing.js';

/**
 * Has every test of the owner in `dir` that installs it with core's test registry install the
 * collector or feature it now depends on too: `<helper>::install(&[collectors], &[features])`,
 * with or without the crate's prefix.
 */
export function installing(
  root: string,
  plan: Plan,
  dir: string,
  dependency: { collector?: string; feature?: string },
  only?: string,
) {
  const [name, list] = dependency.collector ? [dependency.collector, 0] : [dependency.feature!, 1];
  const item = `&dispatch_${name}::${list === 0 ? 'COLLECTOR' : 'FEATURE'}`;
  const there = new RegExp(`\\b(dispatch_)?${name}::${list === 0 ? 'COLLECTOR' : 'FEATURE'}\\b`);
  const written = [...plan.files.keys()].filter(
    (file) => file.startsWith(`${dir}/`) && file.endsWith('.rs'),
  );
  for (const file of [...files(root, dir, /\.rs$/), ...written]) {
    if (only && file !== only) continue;
    const fresh = plan.files.has(file);
    const current = fresh ? plan.files.get(file)! : (plan.changes.get(file) ?? read(root, file));
    const merged = current.replace(
      /\b(\w+)::install\(\s*&\[([^\]]*)\],\s*&\[([^\]]*)\]/g,
      (call, helper: string, collectors: string, features: string) => {
        const lists = [collectors, features];
        if (there.test(lists[list]!)) return call;
        lists[list] = [...items(`[${lists[list]}]`), item].join(', ');
        return `${helper}::install(&[${lists[0]}], &[${lists[1]}]`;
      },
    );
    if (merged !== current) (fresh ? plan.files : plan.changes).set(file, merged);
  }
}
