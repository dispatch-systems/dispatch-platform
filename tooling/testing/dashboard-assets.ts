import fs from 'node:fs';
import path from 'node:path';

type Chunk = { file: string; imports?: string[]; css?: string[] };
export type DashboardAssets = Record<string, Chunk>;

/** Static modules and CSS needed by these entries; optional dynamic imports stay deferred. */
export function dashboardGraphSizes(root: string, manifest: DashboardAssets, entries: string[]) {
  const visited = new Set<string>();
  const scripts = new Set<string>();
  const styles = new Set<string>();
  const visit = (key: string) => {
    if (visited.has(key)) return;
    const chunk = manifest[key];
    if (!chunk) throw new Error(`Missing dashboard entry: ${key}`);
    visited.add(key);
    if (chunk.file.endsWith('.js')) scripts.add(chunk.file);
    for (const stylesheet of chunk.css ?? []) styles.add(stylesheet);
    for (const dependency of chunk.imports ?? []) visit(dependency);
  };
  entries.forEach(visit);
  const sizes = (files: Set<string>) => {
    let raw = 0;
    let transferred = 0;
    for (const file of files) {
      const target = path.join(root, file);
      const size = fs.statSync(target).size;
      raw += size;
      transferred += fs.existsSync(target + '.br') ? fs.statSync(target + '.br').size : size;
    }
    return { raw, transferred };
  };
  return { scripts: sizes(scripts), styles: sizes(styles), files: [...scripts, ...styles].sort() };
}
