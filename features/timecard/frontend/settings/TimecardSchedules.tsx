import { useMemo, useState } from 'react';
import { Globe2, Pencil, Plus } from 'lucide-react';
import {
  DataState,
  DataTable,
  ErrorBox,
  useDataTable,
  type TableColumn,
} from '../../../../core/shell/frontend/ui/index.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { scheduleIssue } from '../../../../core/shell/frontend/runtime/schedule-issues.js';
import type { CollectionSchedule, ScheduleInput } from '../../../../core/collection/api/index.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { nextCollection } from './nextCollection.js';
import { ScheduleEditor } from './ScheduleEditor.js';
import './timecard-schedules.css';
import {
  useSchedules,
  getSchedules,
  setScheduleEnabled,
} from '../../../../core/collection/api/client.js';
import { schedules as served } from '../../api/client.js';

function clock(value: string) {
  const [hour, minute] = value.split(':').map(Number);
  return `${hour! % 12 || 12}:${String(minute).padStart(2, '0')} ${hour! < 12 ? 'AM' : 'PM'}`;
}
function repeat(schedule: ScheduleInput) {
  if (schedule.cadence === 'daily') return `Daily at ${clock(schedule.localTime)}`;
  const minutes = schedule.intervalMinutes ?? 120;
  return minutes % 60 === 0
    ? `Every ${minutes / 60} ${minutes === 60 ? 'hour' : 'hours'}`
    : `Every ${minutes} minutes`;
}
function CollectionLabels({ collection }: Pick<ScheduleInput, 'collection'>) {
  return (
    <div className="schedule-collections">
      {collection !== 'meal_break' && <span className="schedule-tag">Paycom</span>}
      {collection !== 'paycom' && (
        <span className="schedule-tag schedule-tag-meal">Meal Break</span>
      )}
    </div>
  );
}
/**
 * Its sync schedules, its custom section of Settings: each with what it collects, Paycom's
 * timecards, Flex's meal breaks or both, when it runs next, and a switch to pause it; one to
 * edit or add in a sheet.
 */
export function TimecardSchedules({ dspId }: { dspId: string }) {
  const query = useSchedules(served, dspId);
  const { notify } = useFeedback();
  const [editing, setEditing] = useState<CollectionSchedule | null | undefined>();
  const [busyId, setBusyId] = useState<string>();
  const [updated, setUpdated] = useState<CollectionSchedule>();
  const [error, setError] = useState('');
  const schedules = useMemo(
    () =>
      (query.data?.schedules ?? []).map((schedule) =>
        updated?.id === schedule.id && updated.revision > schedule.revision ? updated : schedule,
      ),
    [query.data, updated],
  );
  async function toggle(schedule: CollectionSchedule) {
    setBusyId(schedule.id);
    setError('');
    setUpdated({
      ...schedule,
      enabled: !schedule.enabled,
      nextRun: null,
      revision: schedule.revision + 1,
    });
    try {
      const result = await setScheduleEnabled(
        served,
        schedule.id,
        !schedule.enabled,
        schedule.revision,
      );
      setUpdated(result);
      notify(schedule.enabled ? 'Schedule paused' : 'Schedule enabled');
    } catch (cause) {
      setUpdated(undefined);
      setError(messageOf(cause));
    } finally {
      query.refresh();
      setBusyId(undefined);
    }
  }
  const timezone = query.data?.timezone ?? 'UTC';
  const columns: TableColumn<CollectionSchedule>[] = [
    {
      id: 'schedule',
      header: 'Schedule',
      scope: 'col',
      rowHeader: true,
      value: (schedule) => schedule.name,
      cell: (schedule) => schedule.name,
    },
    {
      id: 'collection',
      header: 'Collection',
      scope: 'col',
      dataLabel: 'Collection',
      value: (schedule) => schedule.collection,
      cell: (schedule) => <CollectionLabels collection={schedule.collection} />,
    },
    {
      id: 'repeat',
      header: 'Repeat',
      scope: 'col',
      dataLabel: 'Repeat',
      value: repeat,
      cell: repeat,
    },
    {
      id: 'next',
      header: 'Next collection',
      scope: 'col',
      dataLabel: 'Next collection',
      className: 'schedule-next',
      value: (schedule) => schedule.nextRun,
      cell: (schedule) => (
        <>
          {schedule.enabled ? nextCollection(schedule.nextRun, timezone) : 'Paused'}
          {schedule.lastError && (
            <small>
              {scheduleIssue(schedule.lastError) ?? 'Collection delayed. Check connections.'}
            </small>
          )}
        </>
      ),
    },
    {
      id: 'status',
      header: 'Status',
      scope: 'col',
      dataLabel: 'Status',
      value: (schedule) => (schedule.enabled ? 'On' : 'Paused'),
      cell: (schedule) => (
        <label className="schedule-toggle">
          <input
            type="checkbox"
            role="switch"
            aria-label={`Enable ${schedule.name}`}
            checked={schedule.enabled}
            disabled={busyId !== undefined}
            onChange={() => void toggle(schedule)}
          />
          <span>{schedule.enabled ? 'On' : 'Paused'}</span>
        </label>
      ),
    },
    {
      id: 'actions',
      header: <span className="sr-only">Actions</span>,
      scope: 'col',
      className: 'schedule-row-actions',
      cell: (schedule) => (
        <button
          className="icon-button"
          aria-label={`Edit ${schedule.name}`}
          onClick={() => setEditing(schedule)}
        >
          <Pencil size={16} aria-hidden="true" />
        </button>
      ),
    },
  ];
  const table = useDataTable({ columns, rows: schedules, rowId: (schedule) => schedule.id });
  return (
    <section className="timecard-schedules" aria-labelledby="timecard-schedules-heading">
      <div className="schedule-section-heading">
        <div>
          <h2 id="timecard-schedules-heading">Sync schedules</h2>
          <span className="schedule-count">{schedules.length}</span>
        </div>
        <div>
          {query.data && (
            <span className="schedule-timezone">
              <Globe2 size={14} aria-hidden="true" />
              DSP time zone · {query.data.timezone}
            </span>
          )}
          <button className="primary" disabled={!query.data} onClick={() => setEditing(null)}>
            <Plus size={16} aria-hidden="true" />
            New schedule
          </button>
        </div>
      </div>
      <ErrorBox message={error || query.error} />
      <DataState data={query.data} failed={Boolean(query.error)}>
        {(data) => (
          <>
            {schedules.length ? (
              <div className="schedule-table-wrap">
                <DataTable table={table} className="schedule-table" caption="Sync schedules" />
              </div>
            ) : (
              <div className="schedule-empty">
                <h3>No schedules</h3>
              </div>
            )}
            {editing !== undefined && (
              <ScheduleEditor
                key={`${editing?.id ?? 'new'}-${editing?.revision ?? 0}`}
                schedule={editing}
                timezone={data.timezone}
                onClose={() => setEditing(undefined)}
                onSaved={(text) => {
                  setEditing(undefined);
                  notify(text);
                  query.refresh();
                }}
                onReload={async () => {
                  const fresh = await getSchedules(served);
                  const schedule = fresh.schedules.find((value) => value.id === editing?.id);
                  setEditing(schedule);
                  query.refresh();
                }}
              />
            )}
          </>
        )}
      </DataState>
    </section>
  );
}
