import { useEffect, useRef, useState } from 'react';
import type { CollectionSchedule, ScheduleCollection, ScheduleInput } from '../api/index.js';
import { ApiError } from '../../shell/frontend/runtime/api.js';
import { previewSchedule, removeSchedule, saveSchedule } from '../api/client.js';
import { messageOf } from '../../shell/frontend/lib/errors.js';
import { time } from '../../shell/frontend/lib/format.js';
import { ErrorBox } from '../../shell/frontend/ui/index.js';
import { useUnsavedEdits } from '../../shell/frontend/runtime/useUnsavedEdits.js';

/** What a new schedule starts as: its name and the time it collects at. */
export type ScheduleDefaults = { name: string; localTime: string };

/** One schedule of one collection, new or as it was read, served under `schedules`. */
export function ScheduleForm({
  schedules,
  collection,
  defaults,
  schedule,
  timezone,
  onSaved,
  onCancel,
  onReload,
}: {
  schedules: string;
  collection: ScheduleCollection;
  defaults: ScheduleDefaults;
  schedule: CollectionSchedule | null;
  timezone: string;
  onSaved: (message: string) => void;
  onCancel: () => void;
  onReload: () => Promise<void>;
}) {
  const [draft, setDraft] = useState<ScheduleInput>(() =>
    schedule
      ? { ...schedule }
      : {
          name: defaults.name,
          collection,
          cadence: 'daily',
          intervalMinutes: null,
          localTime: defaults.localTime,
          enabled: true,
        },
  );
  const initial = useRef(draft);
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
  // Edits leave only through Save or Discard.
  const { leave, asking } = useUnsavedEdits({
    what: 'This schedule',
    dirty,
    saveable: !busy && !stale && Boolean(draft.name.trim()),
    save,
  });
  const scheduleId = schedule?.id;
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
      void previewSchedule(
        schedules,
        {
          cadence,
          intervalMinutes,
          localTime,
          ...(scheduleId ? { scheduleId } : {}),
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
  }, [schedules, cadence, intervalMinutes, localTime, enabled, scheduleId]);
  const edit = <K extends keyof ScheduleInput>(key: K, value: ScheduleInput[K]) =>
    setDraft((old) => ({ ...old, [key]: value }));
  async function save(remove = false): Promise<boolean> {
    if (busy) return false;
    setBusy(true);
    setError('');
    try {
      if (remove && schedule) await removeSchedule(schedules, schedule.id, schedule.revision);
      else
        await saveSchedule(schedules, schedule?.id, {
          name: draft.name.trim(),
          collection,
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
        className="schedule-form"
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
          <div className="schedule-timing">
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
          <label className="schedule-toggle">
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
            <div className="schedule-delete-confirm">
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
      {asking}
    </>
  );
}
