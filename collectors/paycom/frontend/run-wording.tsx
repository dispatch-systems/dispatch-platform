import { AlertTriangle } from 'lucide-react';
import type { JobMetrics } from '../../../core/collection/api/index.js';
import { duration, title } from '../../../core/shell/frontend/lib/format.js';
import type { RunWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Diagnostics words every collection's runs, as it did when Paycom's timecards were the only
// collection: its employees and their daily records, and each employee's timecard read as a page.

function SlowReads({ attempt }: { attempt: JobMetrics }) {
  const reads = attempt.pageReads;
  if (!reads || !(reads.active.length > 0 || reads.failures.length > 0 || reads.slowest[0]))
    return null;
  return (
    <div aria-label="Timecard diagnostics">
      <h3>Slow and failed reads</h3>
      <ul className="run-reads">
        {reads.active.map((page) => (
          <li key={`active-${page.ordinal}`}>
            <AlertTriangle size={16} aria-hidden="true" />
            Employee {page.ordinal} · {title(page.stage)} · {duration(page.elapsedMs)}
            {attempt.outcome !== 'running' && ' at interruption'}
          </li>
        ))}
        {reads.failures.map((page) => (
          <li key={`${page.ordinal}-${page.attempt}`}>
            <AlertTriangle size={16} aria-hidden="true" />
            Employee {page.ordinal}, read {page.attempt} · {title(page.stage)} ·{' '}
            {title(page.error ?? '')} after {duration(page.elapsedMs)}
            {page.documentState && ` · document ${page.documentState}`}
            {page.pendingRequests != null && ` · ${page.pendingRequests} data requests pending`}
          </li>
        ))}
      </ul>
      {reads.slowest[0] && (
        <p className="muted">
          Slowest read: employee {reads.slowest[0].ordinal} · navigation{' '}
          {duration(reads.slowest[0].navigationMs)}, page content{' '}
          {duration(reads.slowest[0].contentMs)}, extraction{' '}
          {duration(reads.slowest[0].extractionMs)}
        </p>
      )}
    </div>
  );
}

export const runWording: RunWording = {
  collected: (attempt) =>
    attempt.employees === null
      ? undefined
      : `${attempt.employees} employees · ${attempt.timecards} daily records`,
  reads: (reads) => [
    [
      'Timecards validated',
      `${reads.completed} · ${reads.direct ?? 0} without rendering · ${reads.spotChecked ?? 0} spot-checked · ${reads.earlyReady ?? 0} ready before full page load`,
    ],
    [
      'Page retries',
      `${reads.retries} · ${reads.recovered} recovered · ${reads.resumed ?? 0} resumed`,
    ],
  ],
  slowReads: SlowReads,
};
