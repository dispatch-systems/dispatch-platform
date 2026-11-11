import fs from 'node:fs';
import path from 'node:path';

/** A feature's build-wide identity and the folder that determines its DSP owner. */
export type FeatureLocation = { name: string; dir: string; owner?: string };

export const validDspCode = (code: string) => /^[a-z0-9]{2,16}$/.test(code);

/** The feature containing a repository-relative path, including files not yet on disk. */
export function featureAt(file: string): FeatureLocation | undefined {
  const parts = file.split('/');
  if (parts[0] !== 'features') return undefined;
  const [, first, code, folder] = parts;
  if (first === 'custom')
    return code && folder && validDspCode(code) && /^[a-z][a-z0-9_]*$/.test(folder)
      ? { name: `${code}_${folder}`, dir: parts.slice(0, 4).join('/'), owner: code }
      : undefined;
  return first ? { name: first, dir: `features/${first}` } : undefined;
}

/** Every feature folder, including an unfinished feature with no manifest yet. */
export function featureLocations(root: string): FeatureLocation[] {
  const dirs = (dir: string) =>
    fs.existsSync(path.join(root, dir))
      ? fs
          .readdirSync(path.join(root, dir), { withFileTypes: true })
          .filter((entry) => entry.isDirectory())
          .map((entry) => `${dir}/${entry.name}`)
      : [];
  const locations = dirs('features')
    .flatMap((dir) => (dir === 'features/custom' ? dirs(dir).flatMap((code) => dirs(code)) : [dir]))
    .map(featureAt)
    .filter((entry) => entry !== undefined);
  const seen = new Set<string>();
  for (const feature of locations) {
    if (seen.has(feature.name)) throw new Error(`More than one feature is named ${feature.name}`);
    seen.add(feature.name);
  }
  return locations.sort((a, b) => a.name.localeCompare(b.name));
}

/** Resolves an existing stable id, or the ordinary location of a new feature. */
export const featureDirectory = (root: string, name: string) =>
  featureLocations(root).find((feature) => feature.name === name)?.dir ?? `features/${name}`;
