import type {
  AuditPhrases,
  AuditWording,
  AuditWords,
} from '../../../core/shell/frontend/runtime/slots.js';
import { paycomColumns } from './paycom.js';

// How Timecard's events and settings read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'paycom.settings_updated': () => ['updated Paycom settings'],
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

export const wording: AuditWording = {
  phrases,
  spoken: ['paycom.settings_updated'],
  fields,
  value,
};
