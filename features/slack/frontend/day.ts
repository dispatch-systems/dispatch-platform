import { dateFormatter } from '../../../core/shell/frontend/lib/date-format.js';

/** The day `at` falls on in `timeZone`, as the card shows it: `Oct 14`. */
export const day = (at: string | number, timeZone: string) =>
  dateFormatter('en-US', { timeZone, month: 'short', day: 'numeric' }).format(new Date(at));
