import type { Page } from '@playwright/test';
import { login } from '../../../shell/tests/support/fixtures.js';
import type { AuditEvent, AuditPage } from '../../api/index.js';

// The platform owner's audit log, on a feed of events the test answers with: what core's own
// tests and the product's wording test share.
let next = 100;
const event = (at: string, action: string, area: AuditEvent['area'], rest: Partial<AuditEvent>) =>
  ({
    id: next--,
    at,
    actorId: 'usr_maria',
    actorName: 'Maria Lopez',
    dspId: 'dsp_1',
    dspName: 'Northline Logistics',
    action,
    detail: '',
    area,
    target: null,
    ref: null,
    changes: [],
    ...rest,
  }) satisfies AuditEvent;
const system = { actorId: null, actorName: 'System' };
const owner = { actorId: 'usr_owner', actorName: 'Platform Owner' };
const events: AuditEvent[] = [
  event('2026-09-16T14:44:00Z', 'collection.completed', 'collections', {
    changes: [
      { field: 'provider', from: null, to: 'paycom' },
      { field: 'date', from: null, to: '2026-09-15' },
      { field: 'duration', from: null, to: '108' },
    ],
  }),
  event('2026-09-16T14:42:00Z', 'collection.requested', 'collections', { detail: '2026-09-15' }),
  event('2026-09-16T11:00:00Z', 'collection.failed', 'collections', {
    ...system,
    detail: 'manual_verification_required',
    target: 'Morning Paycom pull',
    changes: [{ field: 'provider', from: null, to: 'paycom' }],
  }),
  event('2026-09-15T21:38:00Z', 'member.joined', 'team', {
    actorId: 'usr_sam',
    actorName: 'Sam Rivera',
    detail: 'Dispatcher',
    changes: [{ field: 'invitedBy', from: null, to: 'Maria Lopez' }],
  }),
  event('2026-09-15T20:50:00Z', 'member.role_changed', 'team', {
    detail: 'Manager',
    target: 'Jordan Pike',
    ref: { kind: 'member', id: 'usr_jordan' },
    changes: [{ field: 'role', from: 'Dispatcher', to: 'Manager' }],
  }),
  event('2026-09-15T20:45:00Z', 'dsp.view_opened', 'team', {
    actorId: 'usr_sam',
    actorName: 'Sam Rivera',
  }),
  event('2026-09-15T20:12:00Z', 'member.invited', 'team', {
    detail: 'Dispatcher',
    target: 'sam@northline.test',
  }),
  event('2026-09-15T19:05:00Z', 'schedule.updated', 'schedules', {
    detail: 'Morning pull',
    target: 'Morning pull',
    changes: [{ field: 'time', from: '05:30', to: '06:00' }],
  }),
  event('2026-09-15T17:10:00Z', 'role.updated', 'roles', {
    detail: 'Dispatcher',
    target: 'Dispatcher',
    changes: [
      { field: 'permission', from: null, to: 'timecard.manage' },
      { field: 'permission', from: 'members.invite', to: null },
    ],
  }),
  event('2026-09-15T17:05:00Z', 'dsp.settings_updated', 'settings', {
    ...owner,
    changes: [{ field: 'timezone', from: 'America/Chicago', to: 'America/Denver' }],
  }),
  event('2026-09-15T17:02:00Z', 'dsp.owner_view_opened', 'team', owner),
  event('2026-09-15T16:40:00Z', 'dsp.owner_view_opened', 'team', owner),
  event('2026-09-15T16:01:00Z', 'dsp.owner_view_opened', 'team', owner),
  event('2026-09-15T15:30:00Z', 'dsp.profile_completed', 'settings', {
    changes: [
      { field: 'station', from: null, to: 'TST2' },
      { field: 'abbreviation', from: null, to: 'NLOG' },
    ],
  }),
  // Written before events named their subject.
  event('2026-09-15T15:00:00Z', 'member.role_changed', 'team', { detail: 'Member' }),
  event('2026-09-15T14:30:00Z', 'collection.completed', 'collections', system),
  event('2026-09-15T14:00:00Z', 'schedule.deleted', 'schedules', {
    detail: 'schedule_0123456789abcdef0123456789abcdef',
  }),
  event('2026-09-15T13:50:00Z', 'connection.disabled', 'connections', { detail: 'cortex' }),
  event('2026-09-15T13:40:00Z', 'employees.links_updated', 'settings', {
    detail: 'Revision 3; 2 changes',
  }),
  event('2026-09-15T13:38:00Z', 'employees.links_updated', 'settings', {
    detail: 'Revision 4; 3 changes',
    changes: [
      { field: 'linked', from: null, to: '2' },
      { field: 'separated', from: null, to: '1' },
    ],
  }),
  event('2026-09-15T13:36:00Z', 'paycom.settings_updated', 'settings', {
    detail: 'Revision 5',
    changes: [
      { field: 'paycom.automatic_sync', from: 'true', to: 'false' },
      { field: 'paycom.late_da_time', from: '10:01', to: '09:45' },
      { field: 'paycom.department', from: 'All', to: 'Drivers' },
    ],
  }),
  // A retried attempt is not the collection's outcome, so it is not a failure.
  event('2026-09-15T13:34:00Z', 'collection.retrying', 'collections', {
    ...system,
    detail: 'provider_timeout',
    ref: { kind: 'job', id: 'job_1' },
    changes: [
      { field: 'provider', from: null, to: 'cortex' },
      { field: 'attempt', from: null, to: '1 of 3' },
    ],
  }),
  event('2026-09-15T13:32:00Z', 'collection.failed', 'collections', {
    ...system,
    detail: 'provider_timeout',
    ref: { kind: 'job', id: 'job_1' },
    changes: [
      { field: 'provider', from: null, to: 'cortex' },
      { field: 'attempt', from: null, to: '3 of 3' },
    ],
  }),
  // An event this build has no wording for still reads, with its detail.
  event('2026-09-15T13:30:00Z', 'vehicle.inspection_logged', 'settings', { detail: 'Van 12' }),
];

/** The filters each export posted, since the page was last opened. */
export const exports: URLSearchParams[] = [];
/** Opens the audit log on these events, answering its reads; returns the queries it read with. */
export async function open(page: Page) {
  const requests: URLSearchParams[] = [];
  exports.length = 0;
  await page.route(/\/api\/platform\/audit(\/export$|\?)/, (route) => {
    // The page reads with query parameters and exports by posting the same filters.
    const exporting = route.request().method() === 'POST';
    const query = exporting
      ? new URLSearchParams({ ...route.request().postDataJSON(), limit: '100' })
      : new URL(route.request().url()).searchParams;
    if (!exporting) requests.push(query);
    else exports.push(query);
    const area = query.get('area');
    const subject = query.get('subject');
    const matching = events.filter(
      (item) =>
        (!area || (area === 'failures' ? item.action.endsWith('.failed') : item.area === area)) &&
        (!subject || (item.ref && `${item.ref.kind}:${item.ref.id}` === subject)),
    );
    const counts: AuditPage['counts'] = { failures: 1 };
    for (const item of events) counts[item.area] = (counts[item.area] ?? 0) + 1;
    return route.fulfill({
      json: {
        // The first page is short so the log has more to load.
        events: Number(query.get('limit')) > 50 ? matching : matching.slice(0, 14),
        total: matching.length,
        counts,
        actors: [
          { id: 'usr_maria', name: 'Maria Lopez' },
          { id: 'system', name: 'System' },
        ],
        dsps: [{ id: 'dsp_1', name: 'Northline Logistics' }],
      } satisfies AuditPage,
    });
  });
  await login(page);
  if (page.viewportSize()!.width < 700)
    await page.getByRole('button', { name: 'Open navigation' }).click();
  await page.getByRole('link', { name: 'Audit log', exact: true }).click();
  return requests;
}
/** The feed's entries that read as `text`. */
export const item = (page: Page, text: string | RegExp) =>
  page.getByRole('listitem').filter({ hasText: text });
