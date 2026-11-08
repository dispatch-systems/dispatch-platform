import { useEffect, useMemo, useRef, useState } from 'react';
import { CircleCheck, CircleX, Download, Globe, LoaderCircle, Plus } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import type {
  Backup,
  BackupDay,
  BackupRule,
  BackupRuleInput,
  DspBackups,
} from '../../../backups/api/index.js';
import {
  backUpNow,
  downloadBackup,
  previewBackups,
  saveBackupRules,
  switchBackups,
  useDspBackups,
} from '../../../backups/api/client.js';
import { BackupCalendar } from './BackupCalendar.js';
import { BackupRuleEditor } from './BackupRuleEditor.js';
import { finished, newRule, ruleText, until, when, type Draft } from './backupRules.js';
import { ApiError, errorLabel } from '../../../shell/frontend/runtime/api.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { randomId } from '../../../shell/frontend/lib/random-id.js';
import { timezoneName } from '../../../shell/frontend/lib/format.js';
import { Empty, ErrorBox } from '../../../shell/frontend/ui/index.js';
import './backups.css';

// A DSP's Backups tab: when it was last backed up and is next, on or off and Back up now; its
// rules, each on days of the week or dates of the month, how long what it takes is kept and what
// it holds, edited together and saved or discarded together, as they'd run the coming weeks;
// then its backups, newest first, each saved as a zip.

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
  // Read again often while one is being taken, and every minute besides, so one the schedule
  // takes shows without a reload.
  useEffect(() => setPoll(busy ? 3000 : 60_000), [busy]);
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
  const zone = backups.timezone;
  // The rules as read, and the revision they were read at: edits are made against them.
  const [base, setBase] = useState(() => ({
    drafts: draftsOf(backups.rules),
    revision: backups.revision,
  }));
  const [drafts, setDrafts] = useState(base.drafts);
  const [open, setOpen] = useState<string | null>(null);
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

  const saving = useAction(
    async () => {
      try {
        const answer = await saveBackupRules(
          dsp.id,
          rules.map((rule) => rule!),
          base.revision,
        );
        const fresh = { drafts: draftsOf(answer.rules), revision: answer.revision };
        setBase(fresh);
        setDrafts(fresh.drafts);
        setOpen(null);
        setStale(false);
        refresh();
      } catch (cause) {
        // Saved since by another session: read as it is now, to reload the edits from.
        if (cause instanceof ApiError && cause.code === 'backup_schedule_changed') {
          setStale(true);
          refresh();
        }
        throw cause;
      }
    },
    { success: 'Schedule saved', inline: true },
  );
  // Neither changes the rules: edits carry on against the schedule they were read from. The
  // switch shows as turned at once and saves behind it, nothing else held back; turns made while
  // it saves are sent after it, the last one winning, and it shows as turned until the schedule
  // is read again once they are done.
  const [turned, setTurned] = useState<{ on: boolean; read?: DspBackups }>();
  const wanted = useRef<boolean>(undefined);
  const current = useRef(backups);
  current.current = backups;
  const on = turned && (!turned.read || turned.read === backups) ? turned.on : backups.enabled;
  function turn(enabled: boolean) {
    setTurned({ on: enabled });
    const under = wanted.current !== undefined;
    wanted.current = enabled;
    if (under) return;
    void switching.run().then((done) => {
      wanted.current = undefined;
      if (!done) setTurned(undefined);
      refresh();
    });
  }
  const switching = useAction(
    async () => {
      let sent: boolean | undefined;
      while (wanted.current !== sent) {
        sent = wanted.current!;
        await switchBackups(dsp.id, sent);
      }
      setTurned({ on: sent!, read: current.current });
    },
    { inline: true },
  );
  const backingUp = useAction(
    async () => {
      await backUpNow(dsp.id);
      refresh();
    },
    { inline: true },
  );
  // A download asks the owner to confirm it's them first, as it holds the DSP's saved logins.
  const downloading = useAction((id: string) => downloadBackup(dsp.id, id), { inline: true });
  const working = saving.busy;
  const error = saving.error || switching.error || backingUp.error || downloading.error;
  const discard = () => {
    setDrafts(base.drafts);
    setOpen(null);
    setStale(false);
  };
  const reload = () => {
    const fresh = { drafts: draftsOf(backups.rules), revision: backups.revision };
    setBase(fresh);
    setDrafts(fresh.drafts);
    setOpen(null);
    setStale(false);
  };
  const add = () => {
    const rule = newRule(randomId());
    setDrafts((before) => [...before, rule]);
    setOpen(rule.key);
  };
  const change = (rule: Draft) => (next: Draft) =>
    setDrafts((before) => before.map((each) => (each.key === rule.key ? next : each)));

  // The next runs of the rules as they stand, once each is whole, a moment after the last edit.
  const [previewed, setPreviewed] = useState<{ upcoming: string[]; calendar: BackupDay[] }>();
  const asked = dirty && whole && rules.length ? JSON.stringify(rules) : '';
  useEffect(() => {
    setPreviewed(undefined);
    if (!asked) return;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      previewBackups(dsp.id, JSON.parse(asked) as BackupRuleInput[], controller.signal)
        .then((answer) => setPreviewed(answer))
        .catch(() => undefined);
    }, 250);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [dsp.id, asked]);
  const runs = dirty ? previewed : backups;

  const last = backups.backups.find((backup) => !taking(backup));
  const next = backups.upcoming[0];
  const placeOf = (id: string | null) => drafts.findIndex((rule) => rule.id === id);

  return (
    <section className="dsp-backups" aria-label="Backups">
      <header className="backups-head">
        <dl className="backups-stat">
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
                {when(last.finishedAt ?? last.createdAt, zone)}
                {last.size !== null && <small>{size(last.size)}</small>}
              </>
            ) : (
              'None yet'
            )}
          </dd>
        </dl>
        <dl className="backups-stat">
          <dt>Next backup</dt>
          <dd>
            {!backups.configured ? (
              'Not set up'
            ) : next ? (
              <>
                {when(next, zone)}
                <small>{until(next, zone)}</small>
              </>
            ) : backups.rules.length ? (
              'Off'
            ) : (
              'Not scheduled'
            )}
          </dd>
        </dl>
        <div className="backups-head-actions">
          <input
            type="checkbox"
            role="switch"
            aria-label="On schedule"
            checked={on}
            onChange={(e) => turn(e.target.checked)}
          />
          <button
            type="button"
            disabled={backingUp.busy || !backups.configured || backups.backups.some(taking)}
            onClick={() => void backingUp.run()}
          >
            Back up now
          </button>
        </div>
      </header>
      <div className="backups-body">
        <div className="backups-section-head">
          <div>
            <h3>Schedule</h3>
            <span className="backups-zone">
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
            <BackupRuleEditor
              key={rule.key}
              rule={rule}
              place={place}
              open={open === rule.key}
              items={backups.items}
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
        <BackupCalendar days={runs?.calendar} upcoming={runs?.upcoming} timezone={zone} off={!on} />
      )}
      {(dirty || error || readError) && (
        <footer className="backups-actions">
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
                onClick={() => void saving.run()}
              >
                {working ? 'Saving…' : 'Save'}
              </button>
            </>
          )}
        </footer>
      )}
      <div className="backups-history">
        <div className="backups-section-head">
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
                  <span className="backups-when">{when(backup.createdAt, zone)}</span>
                  <span className="backups-size">
                    {backup.size !== null ? size(backup.size) : ''}
                  </span>
                  <span className="backups-by">
                    {backup.status === 'failed' ? (
                      <span className="backups-error">
                        {errorLabel(backup.error ?? '') ?? 'Failed'}
                      </span>
                    ) : rule ? (
                      <span className="backups-tag backups-rule-tag" data-rule={place % 5}>
                        {ruleText(rule)}
                      </span>
                    ) : (
                      <span className="backups-tag">
                        {backup.ruleId ? 'Removed rule' : 'Manual'}
                      </span>
                    )}
                  </span>
                  {backup.status === 'succeeded' && (
                    <button
                      type="button"
                      className="backups-download"
                      disabled={downloading.busy}
                      onClick={() => void downloading.run(backup.id)}
                    >
                      <Download size={14} aria-hidden="true" />
                      Download
                    </button>
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
