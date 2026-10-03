import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';
import type { AuditChange, AuditEvent } from '../../../../shared/contracts/platform-owner.js';
import type { Permission } from '../../../../shared/contracts/accounts.js';
import { errorLabel } from '../../../shell/frontend/runtime/api.js';
import { elapsed, timeOfDay, title } from '../../../shell/frontend/lib/format.js';
import { permissionLabels } from '../../../shell/frontend/runtime/permissions.js';
import { featureCatalog, featureLabel } from '../../../shell/frontend/runtime/features.js';
import {
  collectionLabels,
  type AuditPart,
  type AuditPhrases,
  type AuditWording,
  type AuditWords,
} from '../../../shell/frontend/runtime/slots.js';
import { agentAreas, areaLabels } from '../agents/agents.js';

export const views = new Set(['dsp.view_opened', 'dsp.owner_view_opened']);

// Each owner words its own events through the audit wording slot; the log's loader hands
// their wording over before the log opens. The wording here is the platform's own.
let owners: readonly AuditWording[] = [];
export function installWording(wording: readonly AuditWording[]) {
  owners = wording;
}

// A sentence is built from parts so the feed and the export share one wording.
type Part = AuditPart;
const strong = (value: string): Part => ({ strong: value });
const day = (value: string) =>
  /^\d{4}-\d{2}-\d{2}$/.test(value)
    ? dateFormatter('en-US', { month: 'short', day: 'numeric', timeZone: 'UTC' }).format(
        new Date(`${value}T00:00:00Z`),
      )
    : value;
// Earlier schedule events stored the schedule's id where its name belongs.
const named = (kind: string, name: string | null | undefined): Part[] =>
  name && !/^schedule_[0-9a-f]+$/.test(name) ? [`the ${kind} `, strong(name)] : [`a ${kind}`];
/** A connection by its catalog name. */
const connectionName = (id: string) =>
  featureCatalog.find((entry) => entry.kind === 'connection' && entry.id === id)?.label;

const phrases: AuditPhrases = {
  'member.invited': (e) => ['invited ', strong(e.target ?? 'a new member')],
  'member.joined': () => ['joined the team'],
  'member.role_changed': (e) =>
    e.target ? ['changed ', strong(e.target), '’s role'] : ['changed a member’s role'],
  'member.removed': (e) =>
    e.target ? ['removed ', strong(e.target), ' from the team'] : ['removed a member'],
  'role.created': (e) => ['created ', ...named('role', e.detail)],
  'role.updated': (e) => ['updated ', ...named('role', e.target ?? e.detail)],
  'role.deleted': (e) => ['deleted ', ...named('role', e.detail)],
  'collection.cancelled': () => ['cancelled a collection'],
  'schedule.created': (e) => ['created ', ...named('schedule', e.detail)],
  'schedule.updated': (e) => ['updated ', ...named('schedule', e.target ?? e.detail)],
  'schedule.toggled': (e) => {
    const enabled = e.changes.find((change) => change.field === 'enabled')?.to;
    const verb =
      enabled === 'true' ? 'turned on ' : enabled === 'false' ? 'turned off ' : 'toggled ';
    return [verb, ...named('schedule', e.detail)];
  },
  'schedule.deleted': (e) => ['deleted ', ...named('schedule', e.detail)],
  'connection.credentials_saved': (e) => [
    'saved ',
    strong(connectionName(e.detail) ?? title(e.detail || 'connection')),
    ' credentials',
  ],
  'connection.disabled': (e) => [
    'disconnected ',
    strong(connectionName(e.detail) ?? title(e.detail || 'a connection')),
  ],
  'connection.verification_submitted': (e) => [
    'submitted verification for ',
    strong(connectionName(e.detail) ?? title(e.detail || 'a connection')),
  ],
  'dsp.view_opened': () => ['opened this DSP'],
  'dsp.owner_view_opened': (e) => [
    'opened ',
    ...(support(e) ? ['this DSP'] : [strong(e.dspName ?? 'a DSP')]),
    ...(e.detail ? [' as ', strong(e.detail)] : []),
  ],
  'dsp.support_visibility_changed': (e) => [
    e.detail === 'shown' ? 'showed Platform support to ' : 'hid Platform support from ',
    strong(e.dspName ?? 'a DSP'),
  ],
  'dsp.feature_enabled': (e) => [
    'enabled ',
    strong(featureLabel(e.detail)),
    ' for ',
    strong(e.dspName ?? 'a DSP'),
  ],
  'dsp.feature_disabled': (e) => [
    'disabled ',
    strong(featureLabel(e.detail)),
    ' for ',
    strong(e.dspName ?? 'a DSP'),
  ],
  'dsp.settings_updated': () => ['updated DSP settings'],
  'dsp.profile_completed': () => ['completed the DSP profile'],
  'mail.retried': (e) => ['retried an email', ...(e.target ? [' to ', strong(e.target)] : [])],
  'mail.discarded': (e) => ['discarded an email', ...(e.target ? [' to ', strong(e.target)] : [])],
  'dsp.created': (e) => ['created ', strong(e.dspName ?? 'a DSP')],
  'dsp.removed': (e) => ['removed ', strong(e.dspName ?? 'a DSP')],
  'agent.key_created': (e) => ['created the agent key ', strong(e.target ?? 'a key')],
  'agent.key_updated': (e) => ['changed the agent key ', strong(e.target ?? 'a key')],
  'agent.key_revoked': (e) => ['revoked the agent key ', strong(e.target ?? 'a key')],
  'agent.app_connected': (e) => ['connected the app ', strong(e.target ?? 'an app')],
  'agent.app_updated': (e) => ['changed the connected app ', strong(e.target ?? 'an app')],
  'agent.app_revoked': (e) => ['revoked the connected app ', strong(e.target ?? 'an app')],
  'agent.pairing_opened': () => ['let apps start connecting for 10 minutes'],
  'agent.app_allowed': (e) => ['let ', strong(e.target ?? 'an app'), ' connect'],
  'agent.app_disallowed': (e) => ['stopped ', strong(e.target ?? 'an app'), ' from connecting'],
  'agent.keys_revoked': (e) => [
    'revoked ',
    strong(e.detail === '1' ? 'an agent key' : `${e.detail} agent keys`),
  ],
  'dsp.restored': (e) => ['restored ', strong(e.dspName ?? 'a DSP')],
  'dsp.suspended': (e) => ['suspended ', strong(e.dspName ?? 'a DSP')],
  'dsp.resumed': (e) => ['resumed ', strong(e.dspName ?? 'a DSP')],
  'audit.exported': () => ['exported the audit log'],
  'account.signed_in': () => ['signed in'],
  'account.password_changed': () => ['changed their password'],
  'account.password_reset': () => ['reset their password'],
  'account.passkey_added': () => ['added a passkey'],
  'account.passkey_removed': () => ['removed a passkey'],
  'account.authenticator_added': () => ['added an authenticator app'],
  'account.authenticator_removed': () => ['removed an authenticator app'],
  'account.second_factor_verified': (e) => [
    e.detail === 'authenticator'
      ? 'verified their identity with an authenticator app'
      : 'verified their identity with a passkey',
  ],
  'account.recovery_codes_created': () => ['created new recovery codes'],
  'account.recovery_code_used': () => ['used a recovery code'],
  'account.session_revoked': () => ['signed out a session'],
  'account.other_sessions_revoked': () => ['signed out their other sessions'],
  'account.all_sessions_revoked': () => ['signed out all their sessions'],
  'diagnostics.fixtures_loaded': () => ['loaded demo data'],
  'development.fixtures_loaded': () => ['loaded demo data'],
};
// Outcomes describe the work itself; whoever asked for it moves to the second line.
const outcomes: Record<string, string> = {
  'collection.completed': 'completed',
  'collection.failed': 'failed',
  'collection.retrying': 'failed',
};
// Outcomes and joins carry facts rather than edits; their sentences spell them out.
export const facts = new Set([
  'provider',
  'date',
  'duration',
  'attempt',
  'invitedBy',
  'linked',
  'separated',
  'automatic',
  'reason',
  'openUntil',
]);
const fact = (event: AuditEvent, field: string) =>
  event.changes.find((change) => change.field === field)?.to ?? '';
const collected = (provider: string) =>
  owners.find((owner) => owner.collected?.[provider])?.collected?.[provider];
// Why Dispatch itself ended a connected app.
const appEndings: Record<string, string> = {
  replaced: 'Replaced by a new connection',
  signed_out: 'The app signed out',
  code_reused: 'Its sign-in code was used twice',
  refresh_reused: 'An old token was used again',
  token_reuse: 'An old token was used again',
};
function outcome(event: AuditEvent): Part[] {
  const attempt = event.action === 'collection.retrying' && fact(event, 'attempt');
  const result = attempt
    ? ` attempt ${attempt} ${outcomes[event.action]}`
    : ` ${outcomes[event.action]}`;
  if (event.target) return ['Scheduled collection ', strong(event.target), result];
  const provider = collected(fact(event, 'provider'));
  const date = fact(event, 'date');
  return [
    provider ? `${provider} collection` : 'Collection',
    ...(date ? [' for ', strong(day(date))] : []),
    result,
  ];
}
const failures: Record<string, (provider: string) => string> = {
  manual_verification_required: (p) => `${p} needs verification — sign-in was challenged`,
  verification_expired: () => 'Verification expired before it was completed',
  provider_timeout: (p) => `${p} took too long to respond`,
  connection_required: (p) => `${p} is not connected`,
  roster_not_complete: () => 'The roster was not complete yet',
  job_cancelled: () => 'The collection was cancelled',
  browser_unavailable: () => 'The collection browser was unavailable',
  browser_start_failed: () => 'The collection browser could not start',
  browser_lost: () => 'The collection browser stopped responding',
  browser_closed: () => 'The collection browser stopped responding',
  browser_command_timeout: () => 'The collection browser stopped responding',
};
// These sentences already say what `detail` holds.
const spoken = new Set([
  'role.created',
  'role.updated',
  'role.deleted',
  'collection.failed',
  'collection.retrying',
  'schedule.created',
  'schedule.updated',
  'schedule.toggled',
  'schedule.deleted',
  'audit.exported',
  'connection.credentials_saved',
  'connection.disabled',
  'connection.verification_submitted',
  'dsp.owner_view_opened',
  'dsp.support_visibility_changed',
  'dsp.feature_enabled',
  'dsp.feature_disabled',
]);
export const isSpoken = (action: string) =>
  spoken.has(action) || owners.some((owner) => owner.spoken?.includes(action));

const system = (event: AuditEvent) => !event.actorId && event.actorName === 'System';
// Inside a DSP the server names every platform owner this way.
export const support = (event: AuditEvent) =>
  !event.actorId && event.actorName === 'Platform support';
// Whatever a platform owner did reads quietly, their visits included. A member's visit is theirs to see.
export const quiet = (event: AuditEvent) =>
  support(event) || event.action === 'dsp.owner_view_opened';
const clockTime = (value: string) => timeOfDay(`2000-01-01T${value}:00Z`, 'UTC');
const words: AuditWords = { strong, day, clock: clockTime };
const phraseOf = (action: string) =>
  phrases[action] ?? owners.find((owner) => owner.phrases?.[action])?.phrases?.[action];
export function sentence(event: AuditEvent): Part[] {
  if (outcomes[event.action]) return outcome(event);
  const phrase = phraseOf(event.action)?.(event, words) ?? [title(event.action).toLowerCase()];
  return [quiet(event) ? event.actorName : strong(event.actorName), ' ', ...phrase];
}
export const plain = (parts: Part[]) =>
  parts.map((part) => (typeof part === 'string' ? part : part.strong)).join('');

const fields: Record<string, string> = {
  role: 'Role',
  name: 'Name',
  timezone: 'Timezone',
  collection: 'Collects',
  cadence: 'Repeats',
  interval: 'Every',
  time: 'Time',
  enabled: 'Status',
  cause: 'With',
  abbreviation: 'Abbreviation',
  station: 'Station',
  access: 'Access',
  dsps: 'DSPs',
  expires: 'Expires',
  // What an agent key or app reads, a kind of data each, and the DSPs' own settings.
  ...Object.fromEntries(agentAreas.map((area) => [`reads.${area}`, areaLabels[area]])),
  bypass: 'Bypass features',
  dsp_reads: 'DSP settings',
  // Earlier key changes, from before keys read by kind of data.
  tools: 'Tools',
  locations: 'Addresses and GPS',
};
export const fieldLabel = (field: string) =>
  fields[field] ?? owners.find((owner) => owner.fields?.[field])?.fields?.[field] ?? title(field);
const agentValues: Record<string, string> = {
  read: 'Read only',
  operator: 'Operator',
  full: 'Full',
  essential: 'Essential',
  true: 'On',
  false: 'Off',
  never: 'Never',
};
const agentFields = ['access', 'expires', 'bypass', 'tools', 'locations'];
// A schedule's collection as its collector names it.
const scheduleLabel = (value: string) =>
  collectionLabels().find((collection) => collection.schedule.id === value)?.schedule.label;
function ownerValue(field: string, value: string) {
  for (const owner of owners) {
    const read = owner.value?.(field, value, words);
    if (read !== undefined) return read;
  }
  return undefined;
}
export function changeValue(field: string, value: string) {
  if (field === 'permission') return permissionLabels[value as Permission] ?? title(value);
  if (field === 'enabled') return value === 'true' ? 'On' : 'Off';
  if (field === 'collection')
    return scheduleLabel(value) ?? ownerValue(field, value) ?? title(value);
  if (field === 'cadence') return title(value);
  if (field === 'interval') return `${value} min`;
  const read = ownerValue(field, value);
  if (read !== undefined) return read;
  if (field === 'time' && /^\d{2}:\d{2}$/.test(value)) return clockTime(value);
  if (field === 'expires' && value !== 'never')
    return dateFormatter('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(
      new Date(value),
    );
  if (agentFields.includes(field) || field.startsWith('reads.')) return agentValues[value] ?? value;
  return value;
}
export function changeText(change: AuditChange) {
  const value = (side: string | null) => (side === null ? '' : changeValue(change.field, side));
  if (change.field === 'permission')
    return change.to === null ? `− ${value(change.from)}` : `+ ${value(change.to)}`;
  const label = fieldLabel(change.field);
  if (change.from === null) return `${label} ${value(change.to)}`;
  if (change.to === null) return `${label} ${value(change.from)} removed`;
  return `${label} ${value(change.from)} → ${value(change.to)}`;
}
export const failure = (event: AuditEvent) =>
  event.action.endsWith('.failed') || event.action === 'collection.retrying'
    ? (failures[event.detail]?.(connectionName(fact(event, 'provider')) ?? 'The provider') ??
      errorLabel(event.detail) ??
      (event.detail ? title(event.detail) : ''))
    : '';
// The second line: what changed, why it failed, or who asked.
export function notes(event: AuditEvent, platform: boolean): string[] {
  return [
    ...(platform && event.dspName && !views.has(event.action) && !event.action.startsWith('dsp.')
      ? [event.dspName]
      : []),
    ...(outcomes[event.action] && !system(event) ? [`Requested by ${event.actorName}`] : []),
    ...(event.action === 'collection.completed' && fact(event, 'duration')
      ? [elapsed(Number(fact(event, 'duration')))]
      : []),
    ...(event.action === 'audit.exported' && Number(event.detail)
      ? [
          `${Number(event.detail).toLocaleString('en-US')} ${event.detail === '1' ? 'event' : 'events'}`,
        ]
      : []),
    ...(event.action === 'collection.retrying' ? ['Retrying'] : []),
    ...(event.action === 'collection.failed' && Number.parseInt(fact(event, 'attempt')) > 1
      ? [`After ${Number.parseInt(fact(event, 'attempt'))} attempts`]
      : []),
    ...(fact(event, 'invitedBy') ? [`Invited by ${fact(event, 'invitedBy')}`] : []),
    ...(event.action === 'agent.app_revoked' && fact(event, 'reason')
      ? [appEndings[fact(event, 'reason')] ?? title(fact(event, 'reason'))]
      : []),
    ...owners.flatMap((owner) => owner.notes?.(event) ?? []),
  ];
}
