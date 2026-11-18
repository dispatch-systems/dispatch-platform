import fs from 'node:fs';
import path from 'node:path';
import { build } from 'vite';
import { compressAssets } from './compress-assets.js';
import type { DashboardAssets } from '../testing/dashboard-assets.js';

/** The dashboard's production build in `outDir`, compressed as it is served, and what each of
 * its chunks bundles, from Vite's manifest, which stays outside the served folder. */
export async function buildDashboard(
  outDir: string,
  logLevel?: 'error',
): Promise<DashboardAssets> {
  await build({ logLevel, build: { outDir, manifest: true } });
  const manifest = path.join(outDir, '.vite/manifest.json');
  const assets = JSON.parse(fs.readFileSync(manifest, 'utf8'));
  fs.unlinkSync(manifest);
  fs.rmdirSync(path.dirname(manifest));
  await compressAssets(path.join(outDir, 'assets'));
  return assets;
}
