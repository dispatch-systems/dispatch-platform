import { time } from '../../../../core/shell/frontend/lib/format.js';

export const nextCollection = (value: string | null, timezone: string) =>
  time(value, timezone, 'Not scheduled');
