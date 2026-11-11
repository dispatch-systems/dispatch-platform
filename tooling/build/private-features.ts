import fs from 'node:fs';
import path from 'node:path';
import type { Plugin } from 'vite';
import { featureAt, featureLocations } from '../shared/features.js';

type Audience = { owner?: string; platform?: boolean; feature?: string; styles?: string[] };
const ENTRY = '/custom/';
const VIRTUAL = '\0dispatch-custom:';
const normalized = (file: string) => file.replaceAll('\\', '/').split('?')[0]!;

/** Custom entries are independent build inputs, never an import map in the public app. */
export function privateFeatures(directory: string): Plugin {
  const root = fs.realpathSync(directory);
  const features = featureLocations(root).filter((feature) => feature.owner);
  const byName = new Map(features.map((feature) => [feature.name, feature]));
  const featureOf = (id: string) => {
    if (id.startsWith(VIRTUAL)) return byName.get(id.slice(VIRTUAL.length));
    const file = normalized(id);
    // Vite's emitted assets name their sources relative to its app/frontend root.
    const absolute = path.isAbsolute(file) ? file : path.resolve(root, 'app/frontend', file);
    const relative = path.posix.relative(normalized(root), normalized(absolute));
    return featureAt(relative)?.owner ? featureAt(relative) : undefined;
  };
  const platform = (id: string) =>
    /\/core\/(?:platform_owner\/api\/runtime\.ts|tenancy\/api\/generated\/audit-subjects\.ts)$/.test(
      normalized(id),
    );
  const audienceOf = (ids: readonly string[]): Audience | undefined => {
    const owners = new Set(ids.flatMap((id) => featureOf(id)?.owner ?? []));
    const ownerOnly = ids.some(platform);
    if (owners.size > 1 || (owners.size && ownerOnly))
      throw new Error('A private chunk mixes DSP owners or platform-only code');
    const owner = [...owners][0];
    return owner ? { owner } : ownerOnly ? { platform: true } : undefined;
  };
  return {
    name: 'dispatch-private-features',
    config() {
      return {
        build: {
          rolldownOptions: {
            // These entries are imported by server-supplied URLs, so the public graph does
            // not reference their exports. Keep the manifests and catalogs they expose.
            preserveEntrySignatures: 'allow-extension',
            ...(features.length
              ? {
                  input: {
                    index: path.join(root, 'app/frontend/index.html'),
                    ...Object.fromEntries(
                      features.map((feature) => [
                        `custom-${feature.name.replaceAll('_', '-')}`,
                        `${ENTRY}${feature.name}.js`,
                      ]),
                    ),
                  },
                }
              : {}),
            output: {
              strictExecutionOrder: true,
              codeSplitting: {
                groups: [
                  {
                    name(id) {
                      const feature = featureOf(id);
                      return feature
                        ? `custom-${feature.name.replaceAll('_', '-')}`
                        : platform(id)
                          ? 'owner-runtime'
                          : null;
                    },
                    priority: 50,
                    entriesAware: true,
                    // Group private modules themselves; their public dependencies keep
                    // the normal shared graph rather than inheriting this private group.
                    includeDependenciesRecursively: false,
                  },
                ],
              },
              chunkFileNames(chunk) {
                const audience = audienceOf([...chunk.moduleIds, chunk.facadeModuleId ?? '']);
                if (audience?.owner) return `assets/custom-${audience.owner}-[name]-[hash].js`;
                if (audience?.platform) return 'assets/owner-[name]-[hash].js';
                const owner = /[\\/]([^\\/]+)[\\/]frontend[\\/]index\.tsx?$/.exec(
                  chunk.facadeModuleId ?? '',
                )?.[1];
                // Rolldown can name shared public dependencies after their private entry
                // importers. Those importer names must not escape through public URLs.
                if (chunk.name.includes('custom-') || chunk.name.includes('~'))
                  return 'assets/shared-[hash].js';
                return `assets/${chunk.name === 'frontend' && owner ? owner : '[name]'}-[hash].js`;
              },
              assetFileNames(asset) {
                const audience = audienceOf(asset.originalFileNames);
                return audience?.owner
                  ? `assets/custom-${audience.owner}-[name]-[hash][extname]`
                  : 'assets/[name]-[hash][extname]';
              },
            },
          },
        },
      };
    },
    resolveId(id) {
      if (id.startsWith(ENTRY) && id.endsWith('.js') && byName.has(id.slice(ENTRY.length, -3)))
        return VIRTUAL + id.slice(ENTRY.length, -3);
      return null;
    },
    load(id) {
      if (!id.startsWith(VIRTUAL)) return null;
      const feature = byName.get(id.slice(VIRTUAL.length))!;
      const from = path.join(root, feature.dir, 'frontend/feature.ts');
      const catalog = path.join(root, feature.dir, 'api/generated/private-catalog.ts');
      const manifest = fs.existsSync(from)
        ? `export { feature } from ${JSON.stringify(from)};\n`
        : `export const feature = { name: ${JSON.stringify(feature.name)} };\n`;
      return (
        manifest +
        (fs.existsSync(catalog)
          ? `export * from ${JSON.stringify(catalog)};\n`
          : 'export const peopleKinds = []; export const peopleSources = []; export const peopleFeatures = [];\n')
      );
    },
    generateBundle(_, bundle) {
      const privateAssets: Record<string, Audience> = {};
      const mark = (file: string, audience: Audience) => {
        const previous = privateAssets[`/${file}`];
        if (
          previous &&
          (previous.owner !== audience.owner || previous.platform !== audience.platform)
        )
          throw new Error(`Private asset ${file} belongs to more than one audience`);
        privateAssets[`/${file}`] = { ...previous, ...audience };
      };
      for (const file of Object.values(bundle)) {
        const audience =
          file.type === 'chunk'
            ? audienceOf([...file.moduleIds, file.facadeModuleId ?? ''])
            : audienceOf(file.originalFileNames);
        if (!audience) continue;
        if (file.type === 'chunk') {
          const entry = file.facadeModuleId?.startsWith(VIRTUAL)
            ? file.facadeModuleId.slice(VIRTUAL.length)
            : undefined;
          mark(file.fileName, { ...audience, ...(entry ? { feature: entry } : {}) });
          for (const css of file.viteMetadata?.importedCss ?? []) mark(css, audience);
        } else mark(file.fileName, audience);
      }
      // A public entry may never pull private code or styles into another DSP's browser.
      for (const file of Object.values(bundle)) {
        if (file.type !== 'chunk') continue;
        const from = privateAssets[`/${file.fileName}`];
        for (const dependency of [
          ...file.imports,
          ...file.dynamicImports,
          ...(file.viteMetadata?.importedCss ?? []),
          ...(file.viteMetadata?.importedAssets ?? []),
        ]) {
          const to = privateAssets[`/${dependency}`];
          if (to?.platform && !from && file.dynamicImports.includes(dependency)) continue;
          if (
            to &&
            (!from || (to.platform && !from.platform) || (to.owner && from.owner !== to.owner))
          )
            throw new Error(
              `${file.fileName} imports an asset outside its audience: ${dependency}`,
            );
        }
      }
      for (const [file, entry] of Object.entries(privateAssets)) {
        if (!entry.feature) continue;
        const seen = new Set<string>();
        const styles = new Set<string>();
        const visit = (name: string) => {
          if (seen.has(name)) return;
          seen.add(name);
          const chunk = bundle[name];
          if (chunk?.type !== 'chunk') return;
          for (const css of chunk.viteMetadata?.importedCss ?? []) styles.add(`/${css}`);
          for (const dependency of [...chunk.imports, ...chunk.dynamicImports]) visit(dependency);
        };
        visit(file.slice(1));
        entry.styles = [...styles].filter((style) => privateAssets[style]?.owner === entry.owner);
      }
      this.emitFile({
        type: 'asset',
        fileName: 'private-assets.json',
        source: JSON.stringify(privateAssets) + '\n',
      });
    },
  };
}
