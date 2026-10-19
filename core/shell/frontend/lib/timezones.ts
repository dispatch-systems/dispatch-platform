/** The time zones a DSP may keep its time in: every one the browser knows, and the usual ones. */
export const timezones = [
  ...new Set([
    'America/Los_Angeles',
    'America/Chicago',
    'America/New_York',
    'America/Denver',
    'America/Phoenix',
    'America/Anchorage',
    'Pacific/Honolulu',
    'UTC',
    ...Intl.supportedValuesOf('timeZone'),
  ]),
].sort();
