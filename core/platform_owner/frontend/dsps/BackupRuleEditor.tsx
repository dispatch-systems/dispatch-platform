import {
  SchedulePatterns,
  ScheduleTimes,
  ScheduleRuleTools,
} from '../../../shell/frontend/ui/ScheduleEditor.js';
import type { ReactNode } from 'react';
import type { BackupItem, KeepFor, KeepUnit } from '../../../backups/api/index.js';
import { DayPicker } from '../../../shell/frontend/ui/index.js';
import { UNITS, keepText, ruleText, unitText, type Draft } from './backupRules.js';

// One rule of a DSP's backup schedule: its words, how long it keeps and how much it holds, and
// opened, how its day is made, which days or dates it runs on, when, how long it keeps what it
// takes and what of the DSP it holds, each changing it as it's set.

export function BackupRuleEditor({
  rule,
  place,
  open,
  items,
  disabled,
  change,
  toggle,
  remove,
}: {
  rule: Draft;
  /** Its place among the rules, which colours it as the calendar draws it. */
  place: number;
  open: boolean;
  items: BackupItem[];
  disabled: boolean;
  change: (rule: Draft) => void;
  toggle: () => void;
  remove: () => void;
}) {
  const held = items.filter((item) => item.always || !rule.skipped.includes(item.id)).length;
  return (
    <div className={`backups-rule schedule-rule${open ? ' is-open' : ''}`} data-rule={place % 5}>
      <div className="schedule-rule-head">
        <span className="backups-rule-mark" aria-hidden="true" />
        <strong>{ruleText(rule)}</strong>
        <span className="backups-tag">{keepText(rule)}</span>
        <span className="backups-tag">
          {held === items.length ? 'Everything' : `${held} of ${items.length}`}
        </span>
        <ScheduleRuleTools open={open} disabled={disabled} toggle={toggle} remove={remove} />
      </div>
      {open && (
        <fieldset className="schedule-rule-body" disabled={disabled}>
          <SchedulePatterns rule={rule} change={change} />
          <div className="backups-fields">
            <span className="backups-label">On</span>
            <div className="backups-segments" role="radiogroup" aria-label="On">
              {(
                [
                  ['week', 'Days of the week'],
                  ['month', 'Days of the month'],
                ] as const
              ).map(([on, label]) => (
                <button
                  type="button"
                  role="radio"
                  key={on}
                  aria-checked={rule.on === on}
                  onClick={() => change({ ...rule, on })}
                >
                  {label}
                </button>
              ))}
            </div>
            {rule.on === 'week' ? (
              <>
                <span className="backups-label">Days</span>
                <DayPicker
                  days={rule.days}
                  invalid={!rule.days.length}
                  change={(days) => change({ ...rule, days })}
                />
              </>
            ) : (
              <>
                <span className="backups-label backups-top">Dates</span>
                <Dates rule={rule} change={change} />
              </>
            )}
            <ScheduleTimes rule={rule} change={change} />
            <span className="backups-label backups-top">Keep</span>
            <Keep rule={rule} change={change} />
            <span className="backups-label backups-top">Includes</span>
            <Includes rule={rule} items={items} change={change} />
          </div>
        </fieldset>
      )}
    </div>
  );
}

/** The dates of the month a rule runs on, and its last day. */
function Dates({ rule, change }: { rule: Draft; change: (rule: Draft) => void }) {
  const flip = (date: number) =>
    change({
      ...rule,
      dates: rule.dates.includes(date)
        ? rule.dates.filter((each) => each !== date)
        : [...rule.dates, date],
    });
  const none = !rule.dates.length && !rule.lastDay;
  return (
    <div className="backups-dates" role="group" aria-label="Dates" aria-invalid={none || undefined}>
      {Array.from({ length: 31 }, (_, at) => at + 1).map((date) => (
        <button
          type="button"
          key={date}
          aria-pressed={rule.dates.includes(date)}
          onClick={() => flip(date)}
        >
          {date}
        </button>
      ))}
      <button
        type="button"
        className="backups-last"
        aria-pressed={rule.lastDay}
        onClick={() => change({ ...rule, lastDay: !rule.lastDay })}
      >
        Last day
      </button>
    </div>
  );
}

/** How long a rule keeps what it takes: forever, for a while, or as its last few. */
function Keep({ rule, change }: { rule: Draft; change: (rule: Draft) => void }) {
  const count = (value: string) => Math.min(999, Math.max(1, Math.round(Number(value)) || 1));
  const choose = (keep: KeepFor) =>
    change({
      ...rule,
      keep,
      keepCount: keep === 'forever' ? rule.keepCount : (rule.keepCount ?? 30),
      keepUnit: rule.keepUnit ?? 'day',
    });
  const option = (keep: KeepFor, label: ReactNode) => (
    <label className="backups-keep-option">
      <input
        type="radio"
        name={`${rule.key}-keep`}
        checked={rule.keep === keep}
        onChange={() => choose(keep)}
      />
      {label}
    </label>
  );
  const number = (
    <input
      type="number"
      aria-label="Keep how many"
      min={1}
      max={999}
      required
      value={rule.keepCount ?? ''}
      onChange={(e) => change({ ...rule, keepCount: count(e.target.value) })}
    />
  );
  return (
    <div className="backups-keep" role="radiogroup" aria-label="Keep">
      {option('forever', 'Forever')}
      {option(
        'for',
        <>
          For
          {rule.keep === 'for' ? (
            number
          ) : (
            <span className="backups-keep-text">{rule.keepCount ?? 30}</span>
          )}
          <select
            aria-label="Keep unit"
            disabled={rule.keep !== 'for'}
            value={rule.keepUnit ?? 'day'}
            onChange={(e) => change({ ...rule, keepUnit: e.target.value as KeepUnit })}
          >
            {UNITS.map((unit) => (
              <option key={unit} value={unit}>
                {unitText(unit, rule.keepCount)}
              </option>
            ))}
          </select>
        </>,
      )}
      {option(
        'last',
        <>
          The last
          {rule.keep === 'last' ? (
            number
          ) : (
            <span className="backups-keep-text">{rule.keepCount ?? 30}</span>
          )}
          backups
        </>,
      )}
    </div>
  );
}

/** What of the DSP a rule's backups hold: the DSP's own data always. */
function Includes({
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
  );
}
