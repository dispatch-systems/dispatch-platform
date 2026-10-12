import { useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import type {
  EmployeeSyncSource,
  EmployeeSyncStatus,
  EmployeeTimecardPeriod,
} from '../../../api/index.js';
import { syncEmployeePeriod } from '../../../api/client.js';
import { useData } from '../../../../../core/shell/frontend/runtime/api.js';
import { useAction } from '../../../../../core/shell/frontend/runtime/useAction.js';
import { dataCache } from '../../../../../core/shell/frontend/runtime/data-cache.js';
import { collectionData } from '../../../../../core/shell/frontend/runtime/data-policy.js';
import { randomId } from '../../../../../core/shell/frontend/lib/random-id.js';
import { performancePolicy } from '../../../../../core/shell/frontend/lib/performance-policy.js';
import { Badge, ErrorBox } from '../../../../../core/shell/frontend/ui/index.js';
import { timecardPeriod } from '../../timecard-format.js';
import './employee-period-sync.css';

const samePeriod = (a?: EmployeeTimecardPeriod, b?: EmployeeTimecardPeriod) =>
  !!a && !!b && a.from === b.from && a.to === b.to;

function Source({ name, source }: { name: string; source?: EmployeeSyncSource }) {
  const failed = source?.failed ?? 0;
  const cancelled = source?.cancelled ?? 0;
  const complete = !!source?.total && source.completed === source.total;
  const verification = source?.jobs.some((job) => job.status === 'waiting_verification');
  const label = !source
    ? 'Checking…'
    : failed || cancelled
      ? [failed && `${failed} failed`, cancelled && `${cancelled} cancelled`]
          .filter(Boolean)
          .join(', ')
      : complete
        ? 'Complete'
        : !source.enabled
          ? 'Not connected'
          : verification
            ? 'Verification required'
            : source.total
              ? `${source.progress}%`
              : 'Ready';
  return (
    <span className="employee-period-source" role="status" aria-label={`${name} period sync`}>
      <Badge
        value={
          failed
            ? 'failed'
            : cancelled
              ? 'cancelled'
              : complete
                ? 'succeeded'
                : source?.active
                  ? 'running'
                  : 'pending'
        }
      >
        {name} · {label}
      </Badge>
      {!!source?.total && (
        <span className="muted">
          {source.completed}/{source.total}
        </span>
      )}
    </span>
  );
}

export function EmployeePeriodSync({
  period,
  refreshKey,
  onQueued,
}: {
  period?: EmployeeTimecardPeriod;
  refreshKey: string;
  onQueued: () => void;
}) {
  // Keep the batch visible while the selected person's period request is loading.
  const [remembered, setRemembered] = useState(period);
  if (period && !samePeriod(period, remembered)) setRemembered(period);
  const selected = period ?? remembered;
  const [batch, setBatch] = useState<{ period: EmployeeTimecardPeriod; id: string }>();
  const [revision, setRevision] = useState(0);
  const [collecting, setCollecting] = useState(false);
  const key = `${refreshKey}:${revision}`;
  const query = selected
    ? new URLSearchParams({ from: selected.from, to: selected.to })
    : undefined;
  if (query && batch && samePeriod(batch.period, selected)) query.set('requestId', batch.id);
  const status = useData<EmployeeSyncStatus>(
    query ? `/api/dsp/jobs/employees?${query}` : '',
    collecting ? performancePolicy.activeCollectionPollMs : performancePolicy.recoveryPollMs,
    key,
  );
  const candidate = status.data ?? status.stale;
  const shown =
    samePeriod(candidate?.period, selected) &&
    (!batch || !samePeriod(batch.period, selected) || candidate?.requestId === batch.id)
      ? candidate
      : undefined;
  const ready = !!shown;
  useEffect(() => setCollecting(Boolean(shown?.active)), [shown?.active]);
  const paycomVersion =
    shown?.paycom.jobs
      .filter((job) => job.status === 'succeeded')
      .map((job) => job.id)
      .join(',') ?? '';
  const cortexVersion =
    shown?.cortex.jobs
      .filter((job) => job.status === 'succeeded')
      .map((job) => job.id)
      .join(',') ?? '';
  const scope = selected ? `${selected.from}:${selected.to}` : '';
  useEffect(() => {
    if (scope && ready)
      dataCache.observeVersion(`employee-paycom:${scope}`, paycomVersion, collectionData);
  }, [scope, ready, paycomVersion]);
  useEffect(() => {
    if (scope && ready)
      dataCache.observeVersion(`employee-cortex:${scope}`, cortexVersion, (url) =>
        url.startsWith('/api/dsp/paycom/meal-breaks'),
      );
  }, [scope, ready, cortexVersion]);
  const sync = useAction(
    async () => {
      if (!period) return;
      const captured = { ...period };
      const id = randomId();
      try {
        await syncEmployeePeriod(captured, id);
        setBatch({ period: captured, id });
      } finally {
        setRevision((value) => value + 1);
        onQueued();
      }
    },
    { success: 'Employee period sync queued' },
  );
  const unavailable = !period
    ? 'Loading period'
    : !shown
      ? 'Checking sync status'
      : !shown.paycom.enabled
        ? 'Paycom is not connected'
        : !shown.cortex.enabled
          ? 'Cortex is not connected'
          : !shown.scopeAvailable
            ? 'Station is required'
            : shown.active
              ? 'Collection in progress'
              : '';
  return (
    <div className="employee-period-sync">
      <div className="employee-period-sources">
        <Source name="Paycom" source={shown?.paycom} />
        <Source name="Flex" source={shown?.cortex} />
      </div>
      <button
        type="button"
        disabled={
          !!unavailable ||
          !status.data ||
          status.validatedKey !== key ||
          !!status.error ||
          sync.busy
        }
        title={
          unavailable ||
          (period && `All employees · ${timecardPeriod(period.from, period.to)}`) ||
          undefined
        }
        onClick={() => void sync.run()}
      >
        <RefreshCw size={16} className={sync.busy || shown?.active ? 'spin' : undefined} />
        Sync now
      </button>
      <ErrorBox message={status.error} />
    </div>
  );
}
