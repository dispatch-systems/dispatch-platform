import { useRef, useState } from 'react';
import { Plus } from 'lucide-react';
import type {
  CollectionSchedule,
  CollectionSchedules,
} from '../../../../shared/contracts/schedules.js';
import { api, useData } from '../../../../core/shell/frontend/runtime/api.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import { DataState, Empty, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { ScheduleForm, type Leave } from './ScheduleForm.js';

const endpoint = '/api/dsp/dvic/schedules';
export function CollectionSettings({ onClose }: { onClose: () => void }) {
  const schedules = useData<CollectionSchedules>(endpoint);
  const [editing, setEditing] = useState<CollectionSchedule | null>();
  // While the editor is open, closing the sheet asks about unsaved changes first.
  const leave = useRef<Leave | null>(null);
  const { notify } = useFeedback();
  const toggle = useAction(
    async (schedule: CollectionSchedule) => {
      try {
        await api(endpoint + '/' + schedule.id + '/enabled', {
          enabled: !schedule.enabled,
          revision: schedule.revision,
        });
      } finally {
        schedules.refresh();
      }
    },
    { inline: true },
  );
  return (
    <Modal
      title={
        editing === undefined ? 'Collection settings' : editing ? 'Edit schedule' : 'New schedule'
      }
      variant="sheet"
      onClose={() => (leave.current ? leave.current(onClose) : onClose())}
    >
      <div className="dvic-settings">
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
                    <div className="dvic-schedule-card" key={schedule.id}>
                      <strong>{schedule.name}</strong>
                      <p>
                        {schedule.cadence === 'daily'
                          ? 'Daily'
                          : 'Every ' + schedule.intervalMinutes! / 60 + ' hours'}{' '}
                        · {schedule.localTime} · {data.timezone}
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
                schedule={editing}
                timezone={data.timezone}
                leaveRef={leave}
                onCancel={() => setEditing(undefined)}
                onSaved={(message) => {
                  notify(message);
                  setEditing(undefined);
                  schedules.refresh();
                }}
                onReload={async () => {
                  const latest = await api<CollectionSchedules>(endpoint);
                  setEditing(latest.schedules.find((item) => item.id === editing?.id));
                  schedules.refresh();
                }}
              />
            )
          }
        </DataState>
        {schedules.error && <button onClick={schedules.refresh}>Try again</button>}
      </div>
    </Modal>
  );
}
