import { z } from 'zod/mini';
import { count, milliseconds, text, type Replies } from '../../foundation/api/runtime.js';
import type {
  SupportAttachment,
  SupportDetail,
  SupportEvidence,
  SupportIntake,
  SupportMessage,
  SupportNotification,
  SupportProposal,
  SupportTicket,
} from './index.js';

const ticket = z.object({
  id: text.check(z.minLength(1)),
  kind: z.enum(['bug', 'feature']),
  title: text,
  status: z.enum([
    'triaging',
    'acknowledged',
    'answered',
    'unclear_bug',
    'unclear_feature',
    'investigating',
    'needs_information',
    'awaiting_approval',
    'implementing',
    'verifying',
    'queued',
    'waiting',
    'merged',
    'resolved',
    'denied',
  ]),
  progress: text,
  reporterName: text,
  dspId: z.nullable(text),
  dspName: z.nullable(text),
  proposalVersion: count,
  approvedVersion: z.nullable(count),
  prUrl: z.nullable(text),
  headSha: z.nullable(text),
  mergeSha: z.nullable(text),
  releasedVersion: z.nullable(text),
  createdAt: milliseconds,
  updatedAt: milliseconds,
}) satisfies z.ZodMiniType<SupportTicket>;
const message = z.object({
  id: text,
  author: text,
  role: text,
  body: text,
  private: z.boolean(),
  createdAt: milliseconds,
}) satisfies z.ZodMiniType<SupportMessage>;
export const attachmentSchema = z.object({
  id: text.check(z.minLength(1)),
  name: text,
  mediaType: text,
  size: count,
  private: z.boolean(),
}) satisfies z.ZodMiniType<SupportAttachment>;
const strings = z.array(text);
const proposal = z.object({
  version: count,
  sourceSha: text,
  reproduction: text,
  observations: strings,
  uncertainties: strings,
  analysis: text,
  plan: strings,
  alternatives: strings,
  acceptanceCriteria: strings,
  verification: strings,
  productionVerification: strings,
  affectedAreas: strings,
  risks: strings,
  createdAt: milliseconds,
}) satisfies z.ZodMiniType<SupportProposal>;
const evidence = z.object({
  id: text,
  phase: text,
  sourceSha: text,
  criterion: text,
  steps: strings,
  outcome: text,
  passed: z.boolean(),
  attachmentIds: strings,
  createdAt: milliseconds,
}) satisfies z.ZodMiniType<SupportEvidence>;
const detail = z.object({
  ticket,
  description: text,
  release: text,
  sourceSha: text,
  messages: z.array(message),
  attachments: z.array(attachmentSchema),
  proposals: z.array(proposal),
  evidence: z.array(evidence),
  intake: z.nullable(
    z.object({
      outcome: z.enum(['acknowledged', 'answered', 'unclear_bug', 'unclear_feature']),
      reply: text,
      reason: text,
      findings: strings,
      questions: strings,
      investigationAttempted: z.boolean(),
    }) satisfies z.ZodMiniType<SupportIntake>,
  ),
}) satisfies z.ZodMiniType<SupportDetail>;
const notification = z.object({
  id: text,
  ticketId: text,
  body: text,
  readAt: z.nullable(milliseconds),
  createdAt: milliseconds,
}) satisfies z.ZodMiniType<SupportNotification>;
export const replies: Replies = (route, method) => {
  if (method === 'GET') {
    if (route === '/api/support/tickets' || route === '/api/platform/support/tickets')
      return z.array(ticket);
    if (/^\/api\/support\/tickets\/[^/]+$/.test(route)) return detail;
    if (route === '/api/support/notifications') return z.array(notification);
    if (route === '/api/support/notifications/unread') return z.object({ count });
  } else {
    if (route === '/api/support/notifications/read') return z.object({ ok: z.literal(true) });
    if (route === '/api/support/attachments') return attachmentSchema;
    if (
      route === '/api/support/tickets' ||
      /^\/api\/(?:platform\/)?support\/tickets\/[^/]+\/(?:messages|reopen|decision|resume)$/.test(
        route,
      )
    )
      return detail;
  }
  return undefined;
};
