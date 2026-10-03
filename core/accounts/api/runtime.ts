import { z } from 'zod';
import type { Dsp } from '../../../shared/contracts/generated/Dsp';
import { features } from '../../tenancy/api/index.js';
import {
  permissions,
  type DspView,
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
const userSchema = z.object({
  id: text.min(1),
  email: text.min(1),
  firstName: text,
  lastName: text,
  platformOwner: z.boolean(),
}) satisfies z.ZodType<User>;
const dsp = z.object({
  id: text.min(1),
  name: text,
  environment,
  status: z.enum(['provisioning', 'active', 'suspended', 'failed']),
  timezone: text,
  permanent: z.boolean(),
  revision: count,
  createdAt: text,
}) satisfies z.ZodType<Dsp>;
const profile = z.object({
  abbreviation: text,
  stationCode: text,
  setupRequired: z.boolean(),
  removed: z.boolean(),
  supportVisible: z.boolean(),
}) satisfies z.ZodType<DspProfile>;
const permission = z.enum(permissions);
const feature = z.enum(features);
const dspSummary = dsp
  .extend({
    profile,
    ownerEmail: text.nullable(),
    ownerStatus: z.enum(['active', 'invited', 'missing']),
    paycom: connectionStatus,
    connections: z.record(text, connectionStatus),
    lastCollection: text.nullable(),
    nextCollection: text.nullable(),
    role: text.nullable(),
    features: z.array(feature),
    members: count,
  })
  .passthrough() satisfies z.ZodType<DspSummary>;
const securityStatus = z.object({
  enrolled: z.boolean(),
  required: z.boolean(),
  verified: z.boolean(),
  recent: z.boolean(),
  passkeyCount: count,
  authenticator: z.boolean(),
}) satisfies z.ZodType<SecurityStatus>;
const passkeySummary = z.object({
  id: text.min(1),
  name: text.min(1),
  createdAt: milliseconds,
}) satisfies z.ZodType<PasskeySummary>;
const authenticatorSetup = z.object({
  secret: text.regex(/^[A-Z2-7]{32}$/),
  qrCode: text.startsWith('data:image/svg+xml;base64,'),
}) satisfies z.ZodType<AuthenticatorSetup>;
const accountSession = z.object({
  id: text.min(1),
  current: z.boolean(),
  createdAt: milliseconds,
  expiresAt: milliseconds,
  device: text.nullable(),
}) satisfies z.ZodType<AccountSession>;
const recoveryCodes = z.object({
  codes: z
    .array(text.regex(/^(?:[A-Za-z0-9_.]{4}(?:-[A-Za-z0-9_.]{4}){3}|[A-Za-z0-9_-]{43})$/))
    .max(10),
});
const passkeyOptions = z.object({
  publicKey: z.object({ challenge: text.min(1) }).passthrough(),
});
export const sessionSchema = z.object({
  user: userSchema,
  csrf: text.min(1),
  dsps: z.array(dspSummary),
  development: z.boolean(),
  environment,
  release: text,
  providerMode,
  source: z.object({ version: text.nullable(), commit: text.nullable() }),
  security: securityStatus,
}) satisfies z.ZodType<SessionView>;
const viewRole = z.object({ id: text, name: text, owner: z.boolean() });
const viewSchema = z.object({
  dsp,
  token: text.min(1),
  role: viewRole,
  roles: z.array(viewRole).optional(),
  permissions: z.array(permission),
  features: z.array(feature),
  profile,
}) satisfies z.ZodType<DspView>;
const okSchema = z.object({ ok: z.literal(true) });

/** The replies of sign-in, security, the session and its DSP view, and the list of DSPs. */
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/session') return sessionSchema;
    if (route === '/api/auth/security/status') return securityStatus;
    if (route === '/api/auth/security/passkeys') return z.array(passkeySummary);
    if (route === '/api/auth/security/sessions') return z.array(accountSession);
    if (route === '/api/platform/dsps') return z.array(dspSummary);
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
