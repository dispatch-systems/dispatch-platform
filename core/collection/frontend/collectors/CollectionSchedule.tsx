import { useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { ChevronDown, CircleCheck, CircleX, Globe, Plus, RefreshCw } from 'lucide-react';
import type { ScheduledCollection, ScheduleRule, ScheduleRuleInput } from '../../api/index.js';
import { collectNow, previewRules, saveRules, switchSchedule } from '../../api/client.js';
import { ApiError, errorLabel } from '../../../shell/frontend/runtime/api.js';
import { useFeedback } from '../../../shell/frontend/runtime/feedback.js';
import type { Leave } from '../../../shell/frontend/runtime/leave.js';
import { useUnsavedEdits } from '../../../shell/frontend/runtime/useUnsavedEdits.js';
import { scheduleIssue } from '../../../shell/frontend/runtime/schedule-issues.js';
import { messageOf } from '../../../shell/frontend/lib/errors.js';
import { randomId } from '../../../shell/frontend/lib/random-id.js';
import { timezoneName } from '../../../shell/frontend/lib/format.js';
import { Empty, ErrorBox, Popover } from '../../../shell/frontend/ui/index.js';
import { CollectionIcon } from './CollectionIcon.js';
import { RuleEditor } from './RuleEditor.js';
import { WeekPreview, runDay, until } from './WeekPreview.js';
import { finished, newRule, type Draft } from './rules.js';

// One collection's schedule on the Collectors page: when it last ran and runs next, on or off
// and Collect now, and its rules, edited together and saved or discarded together, as they'd
// run this week and next.

const draftsOf = (rules: ScheduleRule[]): Draft[] =>
  rules.map(({ nextRun: _next, lastError: _error, ...rule }) => ({ ...rule, key: rule.id! }));
const sent = (drafts: Draft[]) => drafts.map(({ key: _key, ...rule }) => rule);
const same = (a: Draft[], b: Draft[]) => JSON.stringify(sent(a)) === JSON.stringify(sent(b));

export function CollectionSchedule({
  collection,
  timezone,
  leaveRef,
  saved,
  refresh,
}: {
  collection: ScheduledCollection;
  timezone: string;
  /** Where the page asks before it shows another collection. */
  leaveRef: RefObject<Leave | null>;
  /** Shows the collection as the server answered a change of it. */
  saved: (collection: ScheduledCollection) => void;
  /** Reads the collections again. */
  refresh: () => void;
}) {
  const { notify } = useFeedback();
  // The rules as read, and the revision they were read at: edits are made against them.
  const [base, setBase] = useState(() => ({
    drafts: draftsOf(collection.rules),
    revision: collection.revision,
  }));
  const [drafts, setDrafts] = useState(base.drafts);
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [stale, setStale] = useState(false);
  const dirty = !same(drafts, base.drafts);
  // A collection read afresh replaces the rules unless they're being edited.
  useEffect(() => {
    if (collection.revision === base.revision || dirty) return;
    const fresh = { drafts: draftsOf(collection.rules), revision: collection.revision };
    setBase(fresh);
    setDrafts(fresh.drafts);
    setStale(false);
    setError('');
  }, [collection, base.revision, dirty]);
  const rules = useMemo(() => drafts.map(finished), [drafts]);
  const whole = rules.every(Boolean);

  async function save(): Promise<boolean> {
    if (busy || !whole) return false;
    setBusy(true);
    setError('');
    try {
      const answer = await saveRules(
        collection.collection,
        rules.map((rule) => rule!),
        base.revision,
      );
      const fresh = { drafts: draftsOf(answer.rules), revision: answer.revision };
      setBase(fresh);
      setDrafts(fresh.drafts);
      setOpen(null);
      saved(answer);
      notify('Schedule saved');
      return true;
    } catch (cause) {
      setError(messageOf(cause));
      const changed = cause instanceof ApiError && cause.code === 'schedule_changed';
      setStale(changed);
      // Saved since by another session: read as it is now, to reload the edits from.
      if (changed) refresh();
      return false;
    } finally {
      setBusy(false);
    }
  }
  const { leave, asking } = useUnsavedEdits({
    what: `${collection.label}’s schedule`,
    dirty,
    saveable: whole && !busy && !stale,
    save,
  });
  useEffect(() => {
    leaveRef.current = leave;
  });
  const discard = () => {
    setDrafts(base.drafts);
    setOpen(null);
    setError('');
    setStale(false);
  };
  const reload = () => {
    const fresh = { drafts: draftsOf(collection.rules), revision: collection.revision };
    setBase(fresh);
    setDrafts(fresh.drafts);
    setOpen(null);
    setError('');
    setStale(false);
  };

  // The next runs of the rules as they stand, once each is whole, a moment after the last edit.
  const [runs, setRuns] = useState<string[]>();
  const asked = whole && rules.length ? JSON.stringify(rules) : '';
  useEffect(() => {
    setRuns(undefined);
    if (!asked) return;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      previewRules(
        collection.collection,
        JSON.parse(asked) as ScheduleRuleInput[],
        controller.signal,
      )
        .then((answer) => setRuns(answer.runs))
        .catch(() => undefined);
    }, 250);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [collection.collection, asked]);

  const switching = useRef(false);
  async function turn(enabled: boolean) {
    if (switching.current) return;
    switching.current = true;
    setBusy(true);
    setError('');
    try {
      const answer = await switchSchedule(collection.collection, enabled, collection.revision);
      // The rules are as they were: edits of them carry on from the new revision.
      setBase((before) => ({ ...before, revision: answer.revision }));
      saved(answer);
    } catch (cause) {
      setError(messageOf(cause));
      if (cause instanceof ApiError && cause.code === 'schedule_changed') refresh();
    } finally {
      switching.current = false;
      setBusy(false);
    }
  }
  async function collect(collects: ScheduledCollection['collection']) {
    setError('');
    try {
      await collectNow(collection.collection, collects, randomId());
      notify(`${collection.label} collection queued`);
      saved({ ...collection, running: true });
    } catch (cause) {
      setError(messageOf(cause));
    }
  }

  const issues = new Map(
    collection.rules.map((rule) => [
      rule.id,
      rule.lastError ? (scheduleIssue(rule.lastError) ?? errorLabel(rule.lastError)) : undefined,
    ]),
  );
  const add = () => {
    const rule = newRule(collection.collection, randomId());
    setDrafts((before) => [...before, rule]);
    setOpen(rule.key);
  };
  return (
    <section className="collectors-detail" aria-label={collection.label}>
      <header className="collectors-head">
        <div className="collectors-ident">
          <span className="collectors-tile">
            <CollectionIcon feature={collection.feature} size={20} />
          </span>
          <div>
            <h2>{collection.label}</h2>
            <p>{collection.collectorLabel}</p>
          </div>
        </div>
        <LastRun collection={collection} timezone={timezone} />
        <dl className="collectors-stat">
          <dt>Next run</dt>
          <dd>
            {collection.enabled && collection.nextRun ? (
              <>
                {runDay(collection.nextRun, timezone)}
                <small>{until(collection.nextRun)}</small>
              </>
            ) : collection.rules.length ? (
              'Off'
            ) : (
              'Not scheduled'
            )}
          </dd>
        </dl>
        <div className="collectors-head-actions">
          {collection.rules.length > 0 && (
            <label className="settings-toggle">
              <input
                type="checkbox"
                role="switch"
                aria-label="On schedule"
                checked={collection.enabled}
                disabled={busy}
                onChange={(e) => void turn(e.target.checked)}
              />
            </label>
          )}
          {collection.canCollect && (
            <CollectNow collection={collection} disabled={collection.running} collect={collect} />
          )}
        </div>
      </header>
      <div className="collectors-body">
        <div className="collectors-section-head">
          <div>
            <h3>Schedule</h3>
            <span className="collectors-zone">
              <Globe size={13} />
              {timezoneName(timezone)}
            </span>
          </div>
          <button type="button" className="text-button" disabled={busy} onClick={add}>
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
              choice={collection.choice}
              issue={collection.enabled ? issues.get(rule.id) : undefined}
              disabled={busy}
              change={(next) =>
                setDrafts((before) => before.map((each) => (each.key === rule.key ? next : each)))
              }
              toggle={() => setOpen(open === rule.key ? null : rule.key)}
              remove={() => setDrafts((before) => before.filter((each) => each.key !== rule.key))}
            />
          ))
        ) : (
          <Empty title="Not scheduled" />
        )}
      </div>
      {drafts.length > 0 && (
        <WeekPreview
          rules={rules}
          timezone={timezone}
          runs={runs}
          off={collection.rules.length > 0 && !collection.enabled}
        />
      )}
      {(dirty || error) && (
        <footer className="collectors-actions">
          <ErrorBox message={error} />
          {stale && (
            <button
              type="button"
              disabled={busy || collection.revision === base.revision}
              onClick={reload}
            >
              Reload schedule
            </button>
          )}
          {dirty && (
            <>
              <button type="button" disabled={busy} onClick={discard}>
                Cancel
              </button>
              <button
                type="button"
                className="primary"
                disabled={busy || !whole || stale}
                onClick={() => void save()}
              >
                {busy ? 'Saving…' : 'Save'}
              </button>
            </>
          )}
        </footer>
      )}
      {asking}
    </section>
  );
}

/** How the collection's latest run ended, or that one is under way. */
function LastRun({ collection, timezone }: { collection: ScheduledCollection; timezone: string }) {
  const last = collection.lastRun;
  return (
    <dl className="collectors-stat">
      <dt>Last run</dt>
      <dd>
        {collection.running ? (
          'Collecting'
        ) : last ? (
          <>
            {last.status === 'succeeded' ? (
              <CircleCheck size={15} className="run-ok" aria-label="Succeeded" />
            ) : (
              <CircleX size={15} className="run-failed" aria-label="Failed" />
            )}
            {runDay(last.at, timezone)}
            {last.status !== 'succeeded' && last.error && (
              <small>{errorLabel(last.error) ?? 'Failed'}</small>
            )}
          </>
        ) : (
          'Never'
        )}
      </dd>
    </dl>
  );
}

/** Collect now: what one scheduled run collects, or for a collection whose rules choose what they read, each choice. */
function CollectNow({
  collection,
  disabled,
  collect,
}: {
  collection: ScheduledCollection;
  disabled: boolean;
  collect: (collects: ScheduledCollection['collection']) => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const run = async (collects: ScheduledCollection['collection']) => {
    setBusy(true);
    await collect(collects).finally(() => setBusy(false));
  };
  if (!collection.choice)
    return (
      <button
        type="button"
        disabled={disabled || busy}
        onClick={() => void run(collection.collection)}
      >
        <RefreshCw size={15} />
        Collect now
      </button>
    );
  return (
    <Popover
      className="collectors-collect"
      label="Collect now"
      triggerClassName={disabled || busy ? 'is-disabled' : undefined}
      trigger={
        <>
          <RefreshCw size={15} />
          Collect now
          <ChevronDown size={14} />
        </>
      }
      anchored
    >
      {collection.choice.options.map((option) => (
        <button
          type="button"
          key={option.collection}
          disabled={disabled || busy}
          onClick={() => void run(option.collection)}
        >
          {option.label}
        </button>
      ))}
    </Popover>
  );
}
