import { z } from 'zod/mini';
import type { DspView } from './index.js';
import { count, type Replies } from '../../foundation/api/runtime.js';
import {
  dsp,
  profile,
  viewRole,
  filled,
  customId,
  publicFeatures,
  publicPermissions,
} from './runtime.js';

const customFeatures = z.object({
  catalog: z.array(
    z.object({
      id: filled,
      label: filled,
      place: count,
      kind: z.enum(['page', 'sub']),
      permissions: z.array(filled),
      requires: z.array(filled),
      page: z.nullable(filled),
      tab: z.boolean(),
      connects: z.boolean(),
    }),
  ),
  permissions: z.array(
    z.object({
      id: filled,
      label: filled,
      group: z.nullable(filled),
      implied: z.array(filled),
      under: z.nullable(filled),
    }),
  ),
  frontends: z.array(
    z.object({ name: filled, url: filled, styles: z.array(filled), place: count }),
  ),
});
const viewSchema = z
  .object({
    dsp,
    token: filled,
    role: viewRole,
    roles: z.optional(z.array(viewRole)),
    permissions: z.array(filled),
    features: z.array(filled),
    custom: z.optional(customFeatures),
    profile,
  })
  .check(
    z.refine((view) => {
      const custom = view.custom;
      const catalog = custom?.catalog ?? [];
      const declared = custom?.permissions ?? [];
      const ids = new Set(catalog.map((feature) => feature.id));
      const grants = new Set([
        ...publicPermissions,
        ...declared.map((permission) => permission.id),
      ]);
      const asset = (url: string, extension: string) =>
        Boolean(
          view.dsp.code &&
          url.startsWith(`/assets/custom-${view.dsp.code}-`) &&
          new RegExp(`^/assets/[A-Za-z0-9_.-]+\\.${extension}$`).test(url),
        );
      return (
        ids.size === catalog.length &&
        new Set(declared.map((permission) => permission.id)).size === declared.length &&
        catalog.every(
          (feature) =>
            customId(feature.id, view.dsp.code) &&
            !publicFeatures.has(feature.id) &&
            feature.permissions.every((permission) => grants.has(permission)) &&
            (feature.kind === 'page'
              ? feature.page === null
              : Boolean(
                  feature.page &&
                  ids.has(feature.page) &&
                  feature.id.startsWith(`${feature.page}.`),
                )),
        ) &&
        declared.every(
          (permission) =>
            customId(permission.id, view.dsp.code) &&
            !publicPermissions.has(permission.id) &&
            permission.implied.every((id) => grants.has(id)) &&
            (!permission.under || grants.has(permission.under)),
        ) &&
        view.features.every((id) => publicFeatures.has(id) || ids.has(id)) &&
        view.permissions.every((id) => grants.has(id)) &&
        (custom?.frontends ?? []).every(
          (entry) =>
            catalog.some((feature) => feature.kind === 'page' && feature.id === entry.name) &&
            view.features.includes(entry.name) &&
            (entry.url === `/custom/${entry.name}.js` || asset(entry.url, 'js')) &&
            entry.styles.every((url) => asset(url, 'css')),
        )
      );
    }),
  ) as z.ZodMiniType<DspView>;
/** Full owner metadata validation, loaded only when the public-only view schema refuses it. */
export const replies: Replies = (route, method) =>
  route === '/api/session/dsp' && method === 'POST' ? viewSchema : undefined;
