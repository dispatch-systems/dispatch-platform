import { CalendarClock, Check, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { Job } from '../../../../core/collection/api/index.js';
import { ScheduleSettings } from '../../../../core/collection/frontend/schedules.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import type { RosterStatus } from '../../api/index.js';
import { collectRoster, rosterSchedules, useRosterStatus } from '../../api/client.js';

const active = (job: Job) => ['queued', 'running', 'waiting_verification'].includes(job.status);
/** How often a running collection's status is read again. */
const POLL_MS = 3000;

/** The collection running, or when the roster was collected and is collected next. */
function said(status: RosterStatus | undefined, running: Job | undefined, timezone: string) {
  if (running) return running.message || 'Collecting';
  if (!status?.collectedAt) return 'Not collected yet';
  const next = status.nextRun ? ` · Next ${time(status.nextRun, timezone)}` : '';
  return `Updated ${time(status.collectedAt, timezone)}${next}`;
}

/** Amazon's roster: when it was collected and is collected next, and the controls that change it. */
export function RosterBar({ timezone }: { timezone: string }) {
  // A collection's status isn't announced: it is read every few seconds while one runs.
  const [watching, setWatching] = useState(false);
  const status = useRosterStatus(watching ? POLL_MS : 0);
  const [scheduling, setScheduling] = useState(false);
  const running = status.data?.jobs.find(active);
  useEffect(() => setWatching(Boolean(running)), [running]);
  const collect = useAction(
    async () => {
      await collectRoster(randomId());
      status.refresh();
    },
    { inline: true },
  );
  const data = status.data;
  const collected = Boolean(data?.collectedAt) && !running;
  return (
    <>
      <div className="driver-roster">
        <span className={`driver-roster-icon${collected ? ' collected' : ''}`}>
          {collected ? (
            <Check size={16} aria-hidden="true" />
          ) : (
            <RefreshCw size={16} aria-hidden="true" className={running ? 'spin' : undefined} />
          )}
        </span>
        <div className="driver-roster-status">
          <b>Amazon roster</b>
          <span>{said(data, running, timezone)}</span>
          <ErrorBox message={collect.error} />
        </div>
        <button onClick={() => setScheduling(true)}>
          <CalendarClock size={15} aria-hidden="true" />
          Schedule
        </button>
        <button
          disabled={!data || Boolean(running) || collect.busy}
          onClick={() => void collect.run()}
        >
          <RefreshCw size={15} aria-hidden="true" />
          Collect now
        </button>
      </div>
      {scheduling && (
        <ScheduleSettings
          endpoint={rosterSchedules}
          collection="associates"
          title="Amazon roster"
          defaults={{ name: 'Daily roster', localTime: '05:00' }}
          onClose={() => {
            setScheduling(false);
            status.refresh();
          }}
        />
      )}
    </>
  );
}
