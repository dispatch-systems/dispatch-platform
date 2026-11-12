import { z } from 'zod/mini';
import type { Dsp } from './generated/Dsp';
import { features } from '../../tenancy/api/index.js';
import {
  permissions,
  type DspView,
  type DspInvitation,
  type DspProfile,
  type DspSummary,
  type SessionView,
  type SecurityStatus,
  type AccountSession,
  type AuthenticatorSetup,
  type PasskeySummary,
  type User,
} from './index.js';
import {
  count,
  environment,
  milliseconds,
  providerMode,
  text,
  type Replies,
} from '../../foundation/api/runtime.js';

const connectionStatus = z.enum([
  'not_connected',
  'ready',
  'signing_in',
  'needs_verification',
  'error',
]);
const filled = text.check(z.minLength(1));
const userSchema = z.object({
  id: filled,
  email: filled,
  firstName: text,
  lastName: text,
  platformOwner: z.boolean(),
}) satisfies z.ZodMiniType<User>;
const dsp = z.object({
  id: filled,
  name: text,
  environment,
  status: z.enum(['provisioning', 'active', 'suspended', 'failed']),
  timezone: text,
  permanent: z.boolean(),
  revision: count,
  createdAt: text,
  code: z.nullable(text),
}) satisfies z.ZodMiniType<Dsp>;
const profile = z.object({
  abbreviation: text,
  stationCode: text,
  setupRequired: z.boolean(),
  supportVisible: z.boolean(),
  aiSupportHidden: z.boolean(),
}) satisfies z.ZodMiniType<DspProfile>;
const publicFeatures = new Set<string>(features);
const publicPermissions = new Set<string>(permissions);
const customId = (id: string, code: string | null) =>
  Boolean(
    code &&
    id.startsWith(`${code}_`) &&
    /^[a-z0-9]+_[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)*$/.test(id),
  );
const dspSummary = z
  .looseObject({
    ...dsp.shape,
    profile,
    ownerEmail: z.nullable(text),
    ownerStatus: z.enum(['active', 'invited', 'missing']),
    connections: z.record(text, connectionStatus),
    lastCollection: z.nullable(text),
    nextCollection: z.nullable(text),
    role: z.nullable(text),
    features: z.array(filled),
    customFeatures: z.optional(z.array(filled)),
    members: count,
  })
  .check(
    z.refine((summary) => {
      const custom = summary.customFeatures ?? [];
      return (
        custom.every((id) => customId(id, summary.code)) &&
        summary.features.every((id) => publicFeatures.has(id) || custom.includes(id))
      );
    }),
  ) satisfies z.ZodMiniType<DspSummary>;
const securityStatus = z.object({
  enrolled: z.boolean(),
  required: z.boolean(),
  verified: z.boolean(),
  recent: z.boolean(),
  passkeyCount: count,
  authenticator: z.boolean(),
}) satisfies z.ZodMiniType<SecurityStatus>;
const passkeySummary = z.object({
  id: filled,
  name: filled,
  createdAt: milliseconds,
}) satisfies z.ZodMiniType<PasskeySummary>;
const authenticatorSetup = z.object({
  secret: text.check(z.regex(/^[A-Z2-7]{32}$/)),
  qrCode: text.check(z.startsWith('data:image/svg+xml;base64,')),
}) satisfies z.ZodMiniType<AuthenticatorSetup>;
const accountSession = z.object({
  id: filled,
  current: z.boolean(),
  createdAt: milliseconds,
  expiresAt: milliseconds,
  device: z.nullable(text),
}) satisfies z.ZodMiniType<AccountSession>;
const dspInvitation = z.object({
  email: text,
  role: text,
  expiresAt: milliseconds,
  accepted: z.union([z.literal(0), z.literal(1)]),
}) satisfies z.ZodMiniType<DspInvitation>;
const recoveryCodes = z.object({
  codes: z
    .array(text.check(z.regex(/^(?:[A-Za-z0-9_.]{4}(?:-[A-Za-z0-9_.]{4}){3}|[A-Za-z0-9_-]{43})$/)))
    .check(z.maxLength(10)),
});
const passkeyOptions = z.object({
  publicKey: z.looseObject({ challenge: filled }),
});
export const sessionSchema = z
  .object({
    user: userSchema,
    csrf: filled,
    dsps: z.array(dspSummary),
    platformFrontend: z.optional(filled),
    development: z.boolean(),
    environment,
    release: text,
    providerMode,
    source: z.object({ version: z.nullable(text), commit: z.nullable(text) }),
    security: securityStatus,
    readOnly: z.boolean(),
  })
  .check(
    z.refine(
      (session) =>
        !session.platformFrontend ||
        (session.user.platformOwner &&
          (!session.security.required || session.security.verified) &&
          (session.platformFrontend === '/custom/platform.js' ||
            /^\/assets\/owner-[A-Za-z0-9_.-]+\.js$/.test(session.platformFrontend))),
    ),
  ) satisfies z.ZodMiniType<SessionView>;
const viewRole = z.object({ id: text, name: text, owner: z.boolean() });
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
const okSchema = z.object({ ok: z.literal(true) });

/** The replies of sign-in, security, the session and its DSP view, and the list of DSPs. */
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/session') return sessionSchema;
    if (route === '/api/auth/security/status') return securityStatus;
    if (route === '/api/auth/security/passkeys') return z.array(passkeySummary);
    if (route === '/api/auth/security/sessions') return z.array(accountSession);
    if (route === '/api/platform/dsps') return z.array(dspSummary);
    if (route === '/api/dsp/invitations') return z.array(dspInvitation);
    return undefined;
  }
  if (
    [
      '/api/auth/login',
      '/api/auth/logout',
      '/api/auth/password',
      '/api/auth/reset-password',
      '/api/auth/forgot-password',
    ].includes(route)
  )
    return okSchema;
  if (route === '/api/auth/security/authenticator/register/start') return authenticatorSetup;
  if (
    route === '/api/auth/security/passkeys/register/start' ||
    route === '/api/auth/security/passkeys/verify/start'
  )
    return passkeyOptions;
  if (
    route === '/api/auth/security/passkeys/register/finish' ||
    route === '/api/auth/security/authenticator/register/finish' ||
    route === '/api/auth/security/recovery-codes'
  )
    return recoveryCodes;
  if (route.startsWith('/api/auth/security/')) return okSchema;
  if (route === '/api/session/dsp') return viewSchema;
  return undefined;
};
