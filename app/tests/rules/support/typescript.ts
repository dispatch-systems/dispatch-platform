import path from 'node:path';
import { moduleSpecifiers, resolveModule } from '../../../../tooling/testing/source-analysis.js';
import { files, isFile, read, root } from './repo.js';

/** An import of one file of the repository by another, both relative to the root. */
export type Import = { file: string; specifier: string; target: string };

const scripts = /\.(tsx?|mts|cts|m?js)$/;
const cache = new Map<string, Import[]>();
/**
 * The repository files a script imports: value, type-only and lazy imports, re-exports, and
 * styles and artwork, which the compiler does not resolve. Packages are not files of ours.
 */
export function imports(file: string): Import[] {
  let found = cache.get(file);
  if (found) return found;
  found = moduleSpecifiers(read(file), file).flatMap((specifier) => {
    if (!specifier.startsWith('.')) return [];
    const resolved = resolveModule(path.join(root, file), specifier);
    const target = resolved
      ? path.relative(root, resolved).split(path.sep).join('/')
      : path.posix.normalize(path.posix.join(path.posix.dirname(file), specifier.split('?')[0]!));
    return [{ file, specifier, target }];
  });
  cache.set(file, found);
  return found;
}
/** Every script of the repository, with what it imports. */
export const scriptFiles = () =>
  files.filter((file) => scripts.test(file) && !file.endsWith('.d.ts') && isFile(file));
