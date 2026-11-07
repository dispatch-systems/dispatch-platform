import { useEffect, useMemo, useRef, useState } from 'react';
import { CircleCheck, CircleX, Download, Globe, LoaderCircle, Plus } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import type {
  Backup,
  BackupItem,
  BackupRule,
  BackupRuleInput,
  DspBackups,
  KeepUnit,
} from '../../../backups/api/index.js';
import {
  backUpNow,
  backupDownload,
  previewBackups,
  saveBackupRules,
  switchBackups,
  useDspBackups,
} from '../../../backups/api/client.js';
import {
  EVERY_DAY,
  RuleEditor,
  WeekPreview,
  finished,
  ruleText,
  runDay,
  until,
} from '../../../collection/frontend/schedule.js';
import { ApiError, errorLabel } from '../../../shell/frontend/runtime/api.js';
import { useFeedback } from '../../../shell/frontend/runtime/feedback.js';
import { messageOf } from '../../../shell/frontend/lib/errors.js';
import { randomId } from '../../../shell/frontend/lib/random-id.js';
import { timezoneName } from '../../../shell/frontend/lib/format.js';
import { Empty, ErrorBox } from '../../../shell/frontend/ui/index.js';
import './backups.css';

// A DSP's Backups tab: when it was last backed up and is next, on or off and Back up now; its
// rules, each when it runs as a collection's rule does, how long what it takes is kept and what
// it holds, edited together and saved or discarded together, as they'd run this week; then its
// backups, newest first, each saved as a zip.

type Draft = BackupRuleInput & { key: string };

const UNITS: KeepUnit[] = ['day', 'week', 'month', 'year'];
const unitText = (unit: KeepUnit, count: number) => `${unit}${count === 1 ? '' : 's'}`;
const draftsOf = (rules: BackupRule[]): Draft[] =>
  rules.map(({ nextRun: _next, ...rule }) => ({ ...rule, key: rule.id ?? randomId() }));
const sent = (drafts: Draft[]) => drafts.map(({ key: _key, ...rule }) => rule);
const same = (a: Draft[], b: Draft[]) => JSON.stringify(sent(a)) === JSON.stringify(sent(b));
const taking = (backup: Backup) => backup.status === 'queued' || backup.status === 'running';

/** "61 MB", "940 KB". */
function size(bytes: number) {
  const units = ['bytes', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${unit ? value.toFixed(value < 10 ? 1 : 0) : value} ${units[unit]}`;
}

export function DspBackupsTab({ dsp }: { dsp: DspSummary }) {
  const [poll, setPoll] = useState(0);
  const { data, error, refresh } = useDspBackups(dsp.id, poll);
  const busy = !!data?.backups.some(taking);
  useEffect(() => setPoll(busy ? 3000 : 0), [busy]);
  if (!data) return <ErrorBox message={error} />;
  return <Backups key={dsp.id} dsp={dsp} backups={data} refresh={refresh} error={error} />;
}

function Backups({
  dsp,
  backups,
  refresh,
  error: readError,
}: {
  dsp: DspSummary;
  backups: DspBackups;
  refresh: () => void;
  error: string;
}) {
  const { notify } = useFeedback();
  const zone = backups.timezone;
  // The rules as read, and the revision they were read at: edits are made against them.
  const [base, setBase] = useState(() => ({
    drafts: draftsOf(backups.rules),
    revision: backups.revision,
  }));
  const [drafts, setDrafts] = useState(base.drafts);
  const [open, setOpen] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState('');
  const [stale, setStale] = useState(false);
  const dirty = !same(drafts, base.drafts);
  // The backups read afresh replace the rules unless they're being edited.
  useEffect(() => {
    if (backups.revision === base.revision || dirty) return;
    const fresh = { drafts: draftsOf(backups.rules), revision: backups.revision };
    setBase(fresh);
    setDrafts(fresh.drafts);
    setStale(false);
  }, [backups, base.revision, dirty]);
  const rules = useMemo(() => drafts.map(finished), [drafts]);
  const whole = rules.every(Boolean);

  async function act(work: () => Promise<DspBackups>, done?: string) {
    setWorking(true);
    setError('');
    try {
      const answer = await work();
      const fresh = { drafts: draftsOf(answer.rules), revision: answer.revision };
      setBase(fresh);
      if (!dirty || done === 'Schedule saved') setDrafts(fresh.drafts);
      if (done) notify(done);
      refresh();
      return true;
    } catch (cause) {
      setError(messageOf(cause));
      const changed = cause instanceof ApiError && cause.code === 'backup_schedule_changed';
      setStale(changed);
      if (changed) refresh();
      return false;
    } finally {
      setWorking(false);
    }
  }
  const save = async () => {
    if (working || !whole) return;
    if (
      await act(
        () =>
          saveBackupRules(
            dsp.id,
            rules.map((rule) => rule!),
            base.revision,
          ),
        'Schedule saved',
      )
    )
      setOpen(null);
  };
  const switching = useRef(false);
  const turn = async (enabled: boolean) => {
    if (switching.current) return;
    switching.current = true;
    // The rules are as they were: edits of them carry on from the new revision.
    await act(async () => {
      const answer = await switchBackups(dsp.id, enabled);
      setBase((before) => ({ ...before, revision: answer.revision }));
      return answer;
    });
    switching.current = false;
  };
  const discard = () => {
    setDrafts(base.drafts);
    setOpen(null);
    setError('');
    setStale(false);
  };
  const reload = () => {
    const fresh = { drafts: draftsOf(backups.rules), revision: backups.revision };
    setBase(fresh);
    setDrafts(fresh.drafts);
    setOpen(null);
    setError('');
    setStale(false);
  };
  const add = () => {
    const rule: Draft = {
      key: randomId(),
      id: null,
      kind: 'times',
      days: EVERY_DAY,
      times: [''],
      every: null,
      from: null,
      to: null,
      keep: 30,
      keepUnit: 'day',
      skipped: ['pictures'],
    };
    setDrafts((before) => [...before, rule]);
    setOpen(rule.key);
  };
  const change = (rule: Draft) => (next: Draft) =>
    setDrafts((before) => before.map((each) => (each.key === rule.key ? next : each)));

  // The next runs of the rules as they stand, once each is whole, a moment after the last edit.
  const [previewed, setPreviewed] = useState<string[]>();
  const asked = dirty && whole && rules.length ? JSON.stringify(rules) : '';
  useEffect(() => {
    setPreviewed(undefined);
    if (!asked) return;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      previewBackups(dsp.id, JSON.parse(asked) as BackupRuleInput[], controller.signal)
        .then((answer) => setPreviewed(answer.upcoming))
        .catch(() => undefined);
    }, 250);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [dsp.id, asked]);
  const runs = dirty ? previewed : backups.upcoming;

  const last = backups.backups.find((backup) => !taking(backup));
  const next = backups.upcoming[0];
  const placeOf = (id: string | null) => drafts.findIndex((rule) => rule.id === id);

  return (
    <section className="collectors-detail dsp-backups" aria-label="Backups">
      <header className="collectors-head">
        <dl className="collectors-stat">
          <dt>Last backup</dt>
          <dd>
            {backups.backups.some(taking) ? (
              <>
                <LoaderCircle size={15} className="backups-spin" aria-hidden="true" />
                Backing up
              </>
            ) : last ? (
              <>
                {last.status === 'succeeded' ? (
                  <CircleCheck size={15} className="run-ok" aria-label="Succeeded" />
                ) : (
                  <CircleX size={15} className="run-failed" aria-label="Failed" />
                )}
                {runDay(last.finishedAt ?? last.createdAt, zone)}
                {last.size !== null && <small>{size(last.size)}</small>}
              </>
            ) : (
              'None yet'
            )}
          </dd>
        </dl>
        <dl className="collectors-stat">
          <dt>Next backup</dt>
          <dd>
            {!backups.configured ? (
              'Not set up'
            ) : next ? (
              <>
                {runDay(next, zone)}
                <small>{until(next, zone)}</small>
              </>
            ) : backups.rules.length ? (
              'Off'
            ) : (
              'Not scheduled'
            )}
          </dd>
        </dl>
        <div className="collectors-head-actions">
          <input
            type="checkbox"
            role="switch"
            aria-label="On schedule"
            checked={backups.enabled}
            disabled={working}
            onChange={(e) => void turn(e.target.checked)}
          />
          <button
            type="button"
            disabled={working || !backups.configured || backups.backups.some(taking)}
            onClick={() => void act(() => backUpNow(dsp.id))}
          >
            Back up now
          </button>
        </div>
      </header>
      <div className="collectors-body">
        <div className="collectors-section-head">
          <div>
            <h3>Schedule</h3>
            <span className="collectors-zone">
              <Globe size={13} />
              {timezoneName(zone)}
            </span>
          </div>
          <button type="button" className="text-button" disabled={working} onClick={add}>
            <Plus size={15} />
            Add rule
          </button>
        </div>
        {drafts.length ? (
          drafts.map((rule, place) => (
            <RuleEditor
              key={rule.key}
              rule={rule}
              place={place}
              open={open === rule.key}
              tags={<Tags rule={rule} items={backups.items} />}
              more={<Kept rule={rule} items={backups.items} change={change(rule)} />}
              disabled={working}
              change={change(rule)}
              toggle={() => setOpen(open === rule.key ? null : rule.key)}
              remove={() => setDrafts((before) => before.filter((each) => each.key !== rule.key))}
            />
          ))
        ) : (
          <Empty title="Not scheduled" />
        )}
      </div>
      {drafts.length > 0 && (
        <WeekPreview rules={rules} timezone={zone} runs={runs} off={!backups.enabled} />
      )}
      {(dirty || error || readError) && (
        <footer className="collectors-actions">
          <ErrorBox message={error || readError} />
          {stale && (
            <button type="button" disabled={working} onClick={reload}>
              Reload schedule
            </button>
          )}
          {dirty && (
            <>
              <button type="button" disabled={working} onClick={discard}>
                Cancel
              </button>
              <button
                type="button"
                className="primary"
                disabled={working || !whole || stale}
                onClick={() => void save()}
              >
                {working ? 'Saving…' : 'Save'}
              </button>
            </>
          )}
        </footer>
      )}
      <div className="backups-history">
        <div className="collectors-section-head">
          <div>
            <h3>Backups</h3>
          </div>
        </div>
        {backups.backups.length ? (
          <ul>
            {backups.backups.map((backup) => {
              const place = placeOf(backup.ruleId);
              const rule = drafts[place];
              return (
                <li key={backup.id}>
                  {taking(backup) ? (
                    <LoaderCircle size={15} className="backups-spin" aria-label="Backing up" />
                  ) : backup.status === 'succeeded' ? (
                    <CircleCheck size={15} className="run-ok" aria-label="Succeeded" />
                  ) : (
                    <CircleX size={15} className="run-failed" aria-label="Failed" />
                  )}
                  <span className="backups-when">{runDay(backup.createdAt, zone)}</span>
                  <span className="backups-size">
                    {backup.size !== null ? size(backup.size) : ''}
                  </span>
                  <span className="backups-by">
                    {backup.status === 'failed' ? (
                      <span className="backups-error">
                        {errorLabel(backup.error ?? '') ?? 'Failed'}
                      </span>
                    ) : rule ? (
                      <span className="collectors-tag backups-rule" data-rule={place % 5}>
                        {ruleText(rule)}
                      </span>
                    ) : (
                      <span className="collectors-tag">
                        {backup.ruleId ? 'Removed rule' : 'Manual'}
                      </span>
                    )}
                  </span>
                  {backup.status === 'succeeded' && (
                    <a
                      className="backups-download"
                      href={backupDownload(dsp.id, backup.id)}
                      download
                    >
                      <Download size={14} aria-hidden="true" />
                      Download
                    </a>
                  )}
                </li>
              );
            })}
          </ul>
        ) : (
          <Empty title="No backups yet" />
        )}
      </div>
    </section>
  );
}

/** How long a rule keeps what it takes, and how much of the DSP it holds, beside its words. */
function Tags({ rule, items }: { rule: Draft; items: BackupItem[] }) {
  const held = items.filter((item) => !rule.skipped.includes(item.id)).length;
  return (
    <>
      <span className="collectors-tag">
        Keep {rule.keep} {unitText(rule.keepUnit, rule.keep)}
      </span>
      <span className="collectors-tag">
        {held === items.length ? 'Everything' : `${held} of ${items.length}`}
      </span>
    </>
  );
}

/** How long a rule keeps what it takes, and what of the DSP it holds. */
function Kept({
  rule,
  items,
  change,
}: {
  rule: Draft;
  items: BackupItem[];
  change: (rule: Draft) => void;
}) {
  const hold = (item: BackupItem, held: boolean) =>
    change({
      ...rule,
      skipped: held
        ? rule.skipped.filter((id) => id !== item.id)
        : [...rule.skipped.filter((id) => id !== item.id), item.id],
    });
  return (
    <>
      <span className="collectors-label">Keep</span>
      <div className="collectors-sentence backups-keep">
        <input
          type="number"
          aria-label="Keep for"
          min={1}
          max={999}
          required
          value={rule.keep}
          onChange={(e) =>
            change({ ...rule, keep: Math.max(1, Math.round(Number(e.target.value))) })
          }
        />
        <select
          aria-label="Keep unit"
          value={rule.keepUnit}
          onChange={(e) => change({ ...rule, keepUnit: e.target.value as KeepUnit })}
        >
          {UNITS.map((unit) => (
            <option key={unit} value={unit}>
              {unitText(unit, rule.keep)}
            </option>
          ))}
        </select>
      </div>
      <span className="collectors-label">Includes</span>
      <div className="backups-items" role="group" aria-label="Includes">
        {items.map((item) => (
          <label key={item.id}>
            <input
              type="checkbox"
              checked={item.always || !rule.skipped.includes(item.id)}
              disabled={item.always}
              onChange={(e) => hold(item, e.target.checked)}
            />
            {item.label}
          </label>
        ))}
      </div>
    </>
  );
}
