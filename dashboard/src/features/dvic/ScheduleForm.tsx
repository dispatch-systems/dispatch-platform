import { useEffect, useRef, useState, type MutableRefObject } from 'react';
import type {
  CollectionSchedule,
  ScheduleInput,
  SchedulePreview,
} from '../../../../shared/contracts/schedules.js';
import { api, ApiError } from '../../app/api.js';
import { messageOf } from '../../lib/errors.js';
import { time } from '../../lib/format.js';
import { ErrorBox, Modal } from '../../ui/index.js';

/** Runs `then` at once, or after the member chooses Save or Discard for unsaved edits. */
export type Leave = (then: () => void) => void;

export function ScheduleForm({
  schedule,
  timezone,
  leaveRef,
  onSaved,
  onCancel,
  onReload,
}: {
  schedule: CollectionSchedule | null;
  timezone: string;
  leaveRef: MutableRefObject<Leave | null>;
  onSaved: (message: string) => void;
  onCancel: () => void;
  onReload: () => Promise<void>;
}) {
  const [draft, setDraft] = useState<ScheduleInput>(() =>
    schedule
      ? { ...schedule }
      : {
          name: 'Daily DVIC',
          collection: 'dvic',
          cadence: 'daily',
          intervalMinutes: null,
          localTime: '18:00',
          enabled: true,
        },
  );
  const initial = useRef(draft);
  const [confirming, setConfirming] = useState<(() => void) | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [stale, setStale] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [preview, setPreview] = useState<string>();
  const [previewError, setPreviewError] = useState('');
  const { cadence, intervalMinutes, localTime, enabled } = draft;
  const dirty = (['name', 'cadence', 'intervalMinutes', 'localTime', 'enabled'] as const).some(
    (key) => draft[key] !== initial.current[key],
  );
  // Edits leave only through Save or Discard; closing the tab drops them silently.
  const leave: Leave = (then) => (dirty ? setConfirming(() => then) : then());
  useEffect(() => {
    leaveRef.current = leave;
    return () => {
      leaveRef.current = null;
    };
  });
  useEffect(() => {
    const controller = new AbortController();
    setPreview(undefined);
    setPreviewError('');
    if (
      !enabled ||
      !/^\d{2}:\d{2}$/.test(localTime) ||
      (cadence === 'interval' &&
        (!intervalMinutes ||
          intervalMinutes < 30 ||
          intervalMinutes > 1440 ||
          intervalMinutes % 30))
    )
      return;
    const timer = setTimeout(() => {
      void api<SchedulePreview>(
        '/api/dsp/dvic/schedules/preview',
        {
          cadence,
          intervalMinutes,
          localTime,
          ...(schedule ? { scheduleId: schedule.id } : {}),
        },
        controller.signal,
      )
        .then((value) => {
          if (!controller.signal.aborted) setPreview(value.nextRun);
        })
        .catch((cause) => {
          if (!controller.signal.aborted) setPreviewError(messageOf(cause));
        });
    }, 200);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [cadence, intervalMinutes, localTime, enabled, schedule?.id]);
  const edit = <K extends keyof ScheduleInput>(key: K, value: ScheduleInput[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  async function save(remove = false): Promise<boolean> {
    if (busy) return false;
    setBusy(true);
    setError('');
    try {
      if (remove && schedule)
        await api('/api/dsp/dvic/schedules/' + schedule.id + '/remove', {
          revision: schedule.revision,
        });
      else
        await api('/api/dsp/dvic/schedules' + (schedule ? '/' + schedule.id : ''), {
          name: draft.name.trim(),
          collection: 'dvic',
          cadence,
          intervalMinutes,
          localTime,
          enabled,
          ...(schedule ? { revision: schedule.revision } : {}),
        });
      onSaved(remove ? 'Schedule deleted' : 'Schedule saved');
      return true;
    } catch (cause) {
      setError(messageOf(cause));
      setStale(
        cause instanceof ApiError &&
          ['schedule_changed', 'schedule_not_found'].includes(cause.code),
      );
      return false;
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <form
        className="dvic-schedule-form"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <fieldset disabled={busy || stale}>
          <label>
            Schedule name
            <input
              required
              maxLength={60}
              value={draft.name}
              onChange={(e) => edit('name', e.target.value)}
            />
          </label>
          <label>
            Frequency
            <select
              value={cadence}
              onChange={(e) =>
                setDraft((old) => ({
                  ...old,
                  cadence: e.target.value as 'daily' | 'interval',
                  intervalMinutes: e.target.value === 'daily' ? null : 120,
                }))
              }
            >
              <option value="daily">Daily</option>
              <option value="interval">Every interval</option>
            </select>
          </label>
          <div className="dvic-schedule-timing">
            {cadence === 'interval' && (
              <label>
                Every (hours)
                <input
                  type="number"
                  min="0.5"
                  max="24"
                  step="0.5"
                  required
                  value={intervalMinutes ? intervalMinutes / 60 : ''}
                  onChange={(e) => edit('intervalMinutes', Number(e.target.value) * 60)}
                />
              </label>
            )}
            <label>
              {cadence === 'daily' ? 'Collection time' : 'Starting at'}
              <input
                type="time"
                required
                value={localTime}
                onChange={(e) => edit('localTime', e.target.value)}
              />
            </label>
          </div>
          <p className="muted">{timezone} · DSP timezone</p>
          <label className="dvic-schedule-toggle">
            <input
              type="checkbox"
              role="switch"
              checked={enabled}
              onChange={(e) => edit('enabled', e.target.checked)}
            />
            Scheduled collection enabled
          </label>
          <p className="muted">
            Next collection:{' '}
            <output>{enabled ? (preview ? time(preview, timezone) : '—') : 'Paused'}</output>
          </p>
        </fieldset>
        <ErrorBox message={previewError} />
        <ErrorBox message={error} />
        {stale && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void onReload().catch((cause) => setError(messageOf(cause)))}
          >
            Reload schedule
          </button>
        )}
        <div className="form-actions">
          <button type="button" disabled={busy} onClick={() => leave(onCancel)}>
            Back
          </button>
          <button type="submit" className="primary" disabled={busy || stale}>
            {busy ? 'Saving…' : 'Save schedule'}
          </button>
        </div>
        {schedule &&
          (deleting ? (
            <div className="dvic-delete-confirm">
              <p>Delete this schedule?</p>
              <button type="button" disabled={busy} onClick={() => setDeleting(false)}>
                Keep schedule
              </button>
              <button
                type="button"
                className="danger"
                disabled={busy || stale}
                onClick={() => void save(true)}
              >
                Delete schedule
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="text-button danger"
              disabled={busy}
              onClick={() => setDeleting(true)}
            >
              Delete schedule
            </button>
          ))}
      </form>
      {confirming && (
        <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
          <p>This schedule has unsaved changes.</p>
          <div className="form-actions">
            <button
              type="button"
              onClick={() => {
                const then = confirming;
                setConfirming(null);
                then();
              }}
            >
              Discard changes
            </button>
            <button
              type="button"
              className="primary"
              disabled={busy || stale || !draft.name.trim()}
              onClick={async () => {
                const then = confirming;
                if (await save()) then();
                else setConfirming(null);
              }}
            >
              Save changes
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
