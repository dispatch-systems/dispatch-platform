import { useState } from 'react';
import { Route } from 'lucide-react';
import type { RouteRetention } from '../../../../shared/contracts/routes.js';
import { setRouteRetention, useRouteRetention } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { dateFormatter } from '../../../../core/shell/frontend/lib/date-format.js';
import { ConfirmDialog, DataState, ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import './routes-settings.css';

// Shortest and longest windows the backend accepts, in days.
const MIN = 30;
const MAX = 3650;
const choices: [string, string][] = [
  ['forever', 'Every day'],
  ['30', '30 days'],
  ['90', '90 days'],
  ['180', '6 months'],
  ['365', '1 year'],
  ['730', '2 years'],
  ['1825', '5 years'],
  ['custom', 'Custom'],
];
const preset = (days: number | null) =>
  days === null ? 'forever' : choices.some(([id]) => id === String(days)) ? String(days) : 'custom';
const longDay = (value: string) =>
  dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    timeZone: 'UTC',
  }).format(new Date(`${value}T00:00:00Z`));
// The first day a window keeps, as the backend counts it: back from today where the DSP is.
function firstKept(days: number, timeZone: string) {
  const today = dateFormatter('en-CA', { timeZone }).format(new Date());
  const start = new Date(`${today}T00:00:00Z`);
  start.setUTCDate(start.getUTCDate() - days);
  return start.toISOString().slice(0, 10);
}

export function RoutesSettings({ timeZone }: { timeZone: string }) {
  const retention = useRouteRetention();
  return (
    <div className="route-data-settings">
      <DataState data={retention.data} error={retention.error} retry={retention.refresh}>
        {(current) => (
          <RetentionPanel current={current} timeZone={timeZone} saved={retention.refresh} />
        )}
      </DataState>
    </div>
  );
}

function RetentionPanel({
  current,
  timeZone,
  saved,
}: {
  current: RouteRetention;
  timeZone: string;
  saved: () => void;
}) {
  const [choice, setChoice] = useState(preset(current.days));
  const [custom, setCustom] = useState(String(current.days ?? 90));
  const [confirming, setConfirming] = useState(false);
  const days = choice === 'forever' ? null : Number(choice === 'custom' ? custom : choice);
  const valid = days === null || (Number.isInteger(days) && days >= MIN && days <= MAX);
  const changed = days !== current.days;
  const start = days === null ? null : firstKept(days, timeZone);
  // Only a window that passes a stored day deletes anything.
  const deletes = start !== null && current.oldestDay !== null && current.oldestDay < start;
  const save = useAction(
    async () => {
      await setRouteRetention(days);
      setConfirming(false);
      saved();
    },
    { success: 'Route data retention saved' },
  );
  return (
    <section className="security-panel route-retention" aria-labelledby="route-retention-title">
      <div className="route-retention-copy">
        <div className="security-panel-heading">
          <Route size={20} aria-hidden="true" />
          <h2 id="route-retention-title">Route data</h2>
        </div>
        <p>
          Collected routes are kept until you choose a window. With one, days older than it are
          deleted automatically within the hour.
        </p>
        <p className="route-retention-held">
          {current.storedDays === 0 || current.oldestDay === null
            ? 'No days stored yet.'
            : `${current.storedDays} ${current.storedDays === 1 ? 'day' : 'days'} stored, from ${longDay(current.oldestDay)}.`}
        </p>
      </div>
      <form
        className="route-retention-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (!valid || !changed) return;
          if (deletes) setConfirming(true);
          else void save.run();
        }}
      >
        <label>
          <span>Keep route data for</span>
          <select value={choice} onChange={(event) => setChoice(event.target.value)}>
            {choices.map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        {choice === 'custom' && (
          <label>
            <span>Days</span>
            <input
              type="number"
              inputMode="numeric"
              min={MIN}
              max={MAX}
              value={custom}
              aria-invalid={!valid}
              aria-describedby="route-retention-range"
              onChange={(event) => setCustom(event.target.value)}
            />
          </label>
        )}
        {choice === 'custom' && (
          <small
            id="route-retention-range"
            className={valid ? undefined : 'route-retention-invalid'}
          >
            From {MIN} to {MAX} days.
          </small>
        )}
        <button type="submit" disabled={!valid || !changed || save.busy}>
          Save
        </button>
        <ErrorBox message={save.error} />
      </form>
      {confirming && start && (
        <ConfirmDialog
          title="Delete older route data?"
          confirm="Delete and save"
          tone="danger"
          busy={save.busy}
          onCancel={() => setConfirming(false)}
          onConfirm={() => void save.run()}
        >
          Route data from before {longDay(start)} will be deleted within the hour. This can’t be
          undone.
        </ConfirmDialog>
      )}
    </section>
  );
}
