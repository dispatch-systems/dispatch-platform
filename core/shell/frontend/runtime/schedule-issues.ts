import { scheduleIssueOf } from './slots.js';

// Why a schedule waits: the collection engine's own reasons, else as its collector or keeper
// words them.
const issues: Record<string, string> = {
  sync_in_progress: 'Waiting for the current collection',
  queue_full: 'Waiting for the collection queue',
};
export const scheduleIssue = (code: string) => issues[code] ?? scheduleIssueOf(code);
