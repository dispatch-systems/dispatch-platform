import { CalendarClock, CircleCheck, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { Job } from '../../../../core/collection/api/index.js';
import { ScheduleSettings } from '../../../../core/collection/frontend/schedules.js';
import { randomId } from '../../../../core/shell/frontend/lib/random-id.js';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import type { DriverCounts } from '../../api/index.js';
import { collectRoster, rosterSchedules, useRosterStatus } from '../../api/client.js';

const active = (job: Job) => ['queued', 'running', 'waiting_verification'].includes(job.status);
/** How often a running collection's status is read again. */
const POLL_MS = 3000;

/** How many people there are and how many wait for a decision, beside Amazon's roster. */
export function RosterBar({
  counts,
  timezone,
  canCollect,
}: {
  counts: DriverCounts;
  timezone: string;
  canCollect: boolean;
}) {
  return (
    <div className="driver-top">
      <div className="driver-stats">
        <div>
          <strong>{counts.people.toLocaleString('en-US')}</strong>
          <span>People</span>
        </div>
        <div className={counts.review ? 'warn' : undefined}>
          <strong>{counts.review.toLocaleString('en-US')}</strong>
          <span>Needs review</span>
        </div>
      </div>
      {canCollect && <Roster timezone={timezone} />}
    </div>
  );
}

/** When the roster was collected and is collected next, and the controls that change it. */
function Roster({ timezone }: { timezone: string }) {
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
  return (
    <>
      <div className="driver-roster">
        <div className="driver-roster-status">
          <b>Amazon roster</b>
          <small>
            {running ? (
              running.message || 'Collecting'
            ) : data?.collectedAt ? (
              <>
                <CircleCheck size={12} aria-hidden="true" />
                {time(data.collectedAt, timezone)}
                {data.nextRun && ` · Next ${time(data.nextRun, timezone)}`}
              </>
            ) : (
              'Not collected yet'
            )}
          </small>
          <ErrorBox message={collect.error} />
        </div>
        <button onClick={() => setScheduling(true)}>
          <CalendarClock size={15} aria-hidden="true" />
          Schedule
        </button>
        <button
          className="primary"
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
