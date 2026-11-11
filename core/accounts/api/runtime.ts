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
export const filled = text.check(z.minLength(1));
const userSchema = z.object({
  id: filled,
  email: filled,
  firstName: text,
  lastName: text,
  platformOwner: z.boolean(),
}) satisfies z.ZodMiniType<User>;
export const dsp = z.object({
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
export const profile = z.object({
  abbreviation: text,
  stationCode: text,
  setupRequired: z.boolean(),
  supportVisible: z.boolean(),
  aiSupportHidden: z.boolean(),
}) satisfies z.ZodMiniType<DspProfile>;
export const publicFeatures = new Set<string>(features);
export const publicPermissions = new Set<string>(permissions);
export const customId = (id: string, code: string | null) =>
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
export const sessionSchema = z.object({
  user: userSchema,
  csrf: filled,
  dsps: z.array(dspSummary),
  development: z.boolean(),
  environment,
  release: text,
  providerMode,
  source: z.object({ version: z.nullable(text), commit: z.nullable(text) }),
  security: securityStatus,
  readOnly: z.boolean(),
}) satisfies z.ZodMiniType<SessionView>;
export const viewRole = z.object({ id: text, name: text, owner: z.boolean() });
// Most DSPs have no custom declarations. Validate those views without loading the
// custom metadata schema; any other view goes through the full validator before admission.
const viewSchema = z.object({
  dsp,
  token: filled,
  role: viewRole,
  roles: z.optional(z.array(viewRole)),
  permissions: z.array(z.enum(permissions)),
  features: z.array(z.enum(features)),
  custom: z.optional(
    z.object({
      catalog: z.array(z.never()),
      permissions: z.array(z.never()),
      frontends: z.array(z.never()),
    }),
  ),
  profile,
}) satisfies z.ZodMiniType<DspView>;
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
