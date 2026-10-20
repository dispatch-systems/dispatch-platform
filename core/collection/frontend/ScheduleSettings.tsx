import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { CollectionSchedule, CollectionSchedules, ScheduleCollection } from '../api/index.js';
import { useData } from '../../shell/frontend/runtime/api.js';
import { getSchedules, setScheduleEnabled } from '../api/client.js';
import { useAction } from '../../shell/frontend/runtime/useAction.js';
import { useFeedback } from '../../shell/frontend/runtime/feedback.js';
import { time } from '../../shell/frontend/lib/format.js';
import { DataState, Empty, ErrorBox } from '../../shell/frontend/ui/index.js';
import { ScheduleForm, type ScheduleChoice, type ScheduleDefaults } from './ScheduleForm.js';

/** Where a collection's schedules are, what each chooses to collect, and how a new one starts. */
type Schedules = {
  endpoint: string;
  collection: ScheduleCollection;
  collects?: ScheduleChoice | null;
  defaults: ScheduleDefaults;
};

/**
 * A collection's schedules, in the page: each with when it runs next, to pause, resume or edit,
 * and a new one to add. A feature serves them under `endpoint`, as core's schedule routes do.
 * Unsaved edits to one are saved or discarded first, before the page moves on.
 */
export function ScheduleSettings(props: Schedules) {
  const [editing, setEditing] = useState<CollectionSchedule | null>();
  return (
    <div className="schedule-settings">
      <ScheduleList {...props} editing={editing} edit={setEditing} />
    </div>
  );
}

/** The schedules, or the one being edited, which `edit` opens and closes. */
function ScheduleList({
  endpoint,
  collection,
  collects,
  defaults,
  editing,
  edit: setEditing,
}: Schedules & {
  editing: CollectionSchedule | null | undefined;
  edit: (schedule: CollectionSchedule | null | undefined) => void;
}) {
  const schedules = useData<CollectionSchedules>(endpoint);
  const { notify } = useFeedback();
  const toggle = useAction(
    async (schedule: CollectionSchedule) => {
      try {
        await setScheduleEnabled(endpoint, schedule.id, !schedule.enabled, schedule.revision);
      } finally {
        schedules.refresh();
      }
    },
    { inline: true },
  );
  return (
    <>
      <DataState data={schedules.data} error={schedules.error} failed={!!schedules.error}>
        {(data) =>
          editing === undefined ? (
            <>
              <button onClick={() => setEditing(null)}>
                <Plus size={16} />
                Add schedule
              </button>
              <ErrorBox message={toggle.error} />
              {data.schedules.length ? (
                data.schedules.map((schedule) => (
                  <div className="schedule-card" key={schedule.id}>
                    <strong>{schedule.name}</strong>
                    <p>
                      {[
                        collects?.options.find(
                          (option) => option.collection === schedule.collection,
                        )?.label,
                        schedule.cadence === 'daily'
                          ? 'Daily'
                          : 'Every ' + schedule.intervalMinutes! / 60 + ' hours',
                        schedule.localTime,
                        data.timezone,
                      ]
                        .filter(Boolean)
                        .join(' · ')}
                    </p>
                    <p className="muted">
                      {schedule.enabled
                        ? 'Next: ' + time(schedule.nextRun, data.timezone, 'Pending')
                        : 'Paused'}
                    </p>
                    {schedule.lastError && <ErrorBox message={schedule.lastError} />}
                    <div>
                      <button
                        disabled={toggle.busy}
                        onClick={() => setEditing(schedule)}
                        aria-label={'Edit ' + schedule.name}
                      >
                        Edit
                      </button>
                      <button
                        disabled={toggle.busy}
                        onClick={() => void toggle.run(schedule)}
                        aria-label={(schedule.enabled ? 'Pause ' : 'Resume ') + schedule.name}
                      >
                        {schedule.enabled ? 'Pause' : 'Resume'}
                      </button>
                    </div>
                  </div>
                ))
              ) : (
                <Empty title="No schedules yet">
                  Add a daily or repeating collection schedule.
                </Empty>
              )}
            </>
          ) : (
            <ScheduleForm
              key={editing ? editing.id + ':' + editing.revision : 'new'}
              schedules={endpoint}
              collection={collection}
              collects={collects}
              defaults={defaults}
              schedule={editing}
              timezone={data.timezone}
              onCancel={() => setEditing(undefined)}
              onSaved={(message) => {
                notify(message);
                setEditing(undefined);
                schedules.refresh();
              }}
              onReload={async () => {
                const latest = await getSchedules(endpoint);
                setEditing(latest.schedules.find((item) => item.id === editing?.id));
                schedules.refresh();
              }}
            />
          )
        }
      </DataState>
      {schedules.error && <button onClick={schedules.refresh}>Try again</button>}
    </>
  );
}
