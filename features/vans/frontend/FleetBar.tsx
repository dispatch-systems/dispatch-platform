import { Check, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { Job } from '../../../core/collection/api/index.js';
import { useCollectionUpdates } from '../../../core/collection/frontend/live-collection.js';
import { randomId } from '../../../core/shell/frontend/lib/random-id.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../core/shell/frontend/ui/index.js';
import type { FleetStatus } from '../api/index.js';
import { collectFleet, useFleetStatus } from '../api/client.js';

const active = (job: Job) => ['queued', 'running', 'waiting_verification'].includes(job.status);
/** How often a running collection's status is read again. */
const POLL_MS = 3000;

/** The collection running, or when the fleet was collected and is collected next. */
function said(status: FleetStatus | undefined, running: Job | undefined, timezone: string) {
  if (running) return running.message || 'Collecting';
  if (!status?.collectedAt) return 'Not collected yet';
  const next = status.nextRun ? ` · Next ${time(status.nextRun, timezone)}` : '';
  return `Updated ${time(status.collectedAt, timezone)}${next}`;
}

/** Amazon's fleet: when it was collected and is collected next, and Collect now. */
export function FleetBar({ timezone }: { timezone: string }) {
  // It follows collections' ends itself, wherever it's drawn, as a schedule's run elsewhere
  // ends; not their progress, which is read every few seconds while one runs.
  useCollectionUpdates();
  const [watching, setWatching] = useState(false);
  const status = useFleetStatus(watching ? POLL_MS : 0);
  const running = status.data?.jobs.find(active);
  useEffect(() => setWatching(Boolean(running)), [running]);
  const collect = useAction(
    async () => {
      await collectFleet(randomId());
      status.refresh();
    },
    { inline: true },
  );
  const data = status.data;
  const collected = Boolean(data?.collectedAt) && !running;
  return (
    <div className="vans-fleet">
      <span className={`vans-fleet-icon${collected ? ' collected' : ''}`}>
        {collected ? (
          <Check size={16} aria-hidden="true" />
        ) : (
          <RefreshCw size={16} aria-hidden="true" className={running ? 'spin' : undefined} />
        )}
      </span>
      <div className="vans-fleet-status">
        <b>Amazon fleet</b>
        <span>{said(data, running, timezone)}</span>
        <ErrorBox message={collect.error} />
      </div>
      <button
        disabled={!data || Boolean(running) || collect.busy}
        onClick={() => void collect.run()}
      >
        <RefreshCw size={15} aria-hidden="true" />
        Collect now
      </button>
    </div>
  );
}
