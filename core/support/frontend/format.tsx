import type { SupportTicket } from '../api/index.js';

export const statusLabels: Record<string, string> = {
  triaging: 'Reviewing message',
  acknowledged: 'Acknowledged',
  answered: 'Answered',
  investigating: 'Investigating',
  needs_information: 'Needs information',
  unclear_bug: 'Unclear Bug Report',
  unclear_feature: 'Unclear Feature',
  awaiting_approval: 'Ready for review',
  implementing: 'Implementing',
  verifying: 'Verifying',
  queued: 'In merge queue',
  waiting: 'Waiting',
  merged: 'Awaiting production release',
  resolved: 'Resolved',
  denied: 'Denied',
};
export const kindLabel = (kind: string) => (kind === 'bug' ? 'Bug report' : 'Feature request');
export const when = (value: number) =>
  new Date(value).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
export function TicketStatus({ ticket }: { ticket: SupportTicket }) {
  return (
    <span className={`support-status support-status-${ticket.status}`}>
      <i aria-hidden="true" />
      {statusLabels[ticket.status] ?? ticket.status}
    </span>
  );
}
