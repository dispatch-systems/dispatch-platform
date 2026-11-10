import type { Connection } from '../../../core/collection/api/index.js';
import type { CheckWording } from '../../../core/collection/frontend/index.js';
import { title } from '../../../core/shell/frontend/lib/format.js';

/** The functions of Paycom's API a collection reads through, as Paycom's API Setup names them. */
export const functions = [
  { id: 'employee_directory', label: 'Employee Directory' },
  { id: 'employee', label: 'Employee' },
  { id: 'punch_history', label: 'Punch History' },
  { id: 'labor_allocation_categories', label: 'Labor Allocation Categories' },
] as const;

/** Why a check of a connection through the API failed, as the card says it under its name. */
function failure(error: string, connection: Connection) {
  switch (error) {
    case 'paycom_api_refused':
      return `Refused. Check them, and that ${connection.sourceAddress ?? "Dispatch's address"} is allowed.`;
    case 'paycom_api_function_off':
      return 'Not turned on in Paycom';
    case 'paycom_station_missing':
      return 'No category named Delivery Station Code';
    case 'paycom_period_required':
      return 'Needs the first day of a pay period';
    case 'paycom_api_unreadable':
      return "Paycom's answer couldn't be read";
    default:
      return title(error);
  }
}

/** How the card words what a connection through the API asked of Paycom as it connected. */
export const checks: CheckWording = {
  labels: {
    credentials: 'SID and Token',
    ...Object.fromEntries(functions.map((each) => [each.id, each.label])),
    pay_period: 'Pay period',
  },
  // A DSP whose timecards were collected before has one already.
  quiet: ['pay_period'],
  failure,
};
