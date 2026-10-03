import type { AuditEvent } from '../../../shared/contracts/index.js';
import type {
  AuditPhrases,
  AuditWording,
  AuditWords,
} from '../../../core/shell/frontend/runtime/slots.js';
import { paycomColumns } from './paycom.js';

// How Timecard's events and settings read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'paycom.settings_updated': () => ['updated Paycom settings'],
  'collection.requested': (e, { strong, day }) => [
    'started a Paycom collection',
    ...(e.detail ? [' for ', strong(day(e.detail))] : []),
  ],
  'cortex.collection.requested': () => ['started a Cortex meal break collection'],
  'meal_breaks.sync_requested': (e, { strong, day }) => [
    'started a meal break sync',
    ...(e.detail ? [' for ', strong(day(e.detail))] : []),
  ],
  // The Meal Breaks page's link editor, before Driver Match.
  'employees.links_updated': () => ['updated employee links'],
};
const fields: Record<string, string> = {
  'paycom.automatic_sync': 'Automatic sync',
  'paycom.sync_interval_seconds': 'Sync every',
  'paycom.opening_page': 'Opening page',
  'paycom.rows_per_page': 'Rows per page',
  'paycom.name_order': 'Name order',
  'paycom.default_sort': 'Default sort',
  'paycom.department': 'Department',
  'paycom.station': 'Station',
  'paycom.columns': 'Columns',
  'paycom.driver_departments': 'Driver departments',
  'paycom.late_da_time': 'Late DA time',
  'paycom.late_da_departments': 'Late DA departments',
};
const paycomValues: Record<string, string> = {
  true: 'On',
  false: 'Off',
  timecards: 'Timecards',
  'meal-breaks': 'Meal Breaks',
  employees: 'Employees',
  first_last: 'First Last',
  last_first: 'Last, First',
  employeeName: 'Name',
  condition: 'Punch status',
  inDay: 'Clock in',
};
function value(field: string, value: string, { clock }: AuditWords) {
  // A schedule of both of its sources.
  if (field === 'collection' && value === 'both') return 'Paycom and meal breaks';
  if (field === 'paycom.sync_interval_seconds' && Number(value))
    return `${Math.round(Number(value) / 60)} min`;
  if (field === 'paycom.late_da_time' && /^\d{2}:\d{2}$/.test(value)) return clock(value);
  if (field === 'paycom.columns')
    return value
      .split(', ')
      .map((column) => paycomColumns.find(([key]) => key === column)?.[1] ?? column)
      .join(', ');
  if (field.startsWith('paycom.')) return paycomValues[value] ?? value;
  return undefined;
}

const fact = (event: AuditEvent, field: string) =>
  event.changes.find((change) => change.field === field)?.to ?? '';
// Link saves say how many drivers were linked, kept apart or handed back to
// automatic matching. Earlier ones kept only "Revision 3; 2 changes".
function linked(event: AuditEvent) {
  const parts = [
    [fact(event, 'linked'), 'linked'],
    [fact(event, 'separated'), 'kept separate'],
    [fact(event, 'automatic'), 'set to automatic'],
  ].flatMap(([count, label]) => (count ? [`${count} ${label}`] : []));
  if (parts.length) return parts;
  const count = Number(/(\d+) changes?$/.exec(event.detail)?.[1]);
  return count ? [`${count} ${count === 1 ? 'link' : 'links'} changed`] : [];
}

export const wording: AuditWording = {
  phrases,
  spoken: [
    'paycom.settings_updated',
    'collection.requested',
    'meal_breaks.sync_requested',
    'employees.links_updated',
  ],
  fields,
  value,
  notes: (event) => (event.action === 'employees.links_updated' ? linked(event) : []),
};
