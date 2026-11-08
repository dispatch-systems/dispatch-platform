import { Check, ChevronUp, Pencil, Plus, Trash2, X } from 'lucide-react';
import type { ReactNode } from 'react';
import type { BackupItem, KeepFor, KeepUnit } from '../../../backups/api/index.js';
import { DayPicker } from '../../../shell/frontend/ui/index.js';
import {
  EVERY,
  MAX_TIMES,
  PATTERNS,
  UNITS,
  duration,
  keepText,
  patternOf,
  ruleText,
  unitText,
  withPattern,
  type Draft,
  type Pattern,
} from './backupRules.js';

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
  const pattern = patternOf(rule);
  const held = items.filter((item) => item.always || !rule.skipped.includes(item.id)).length;
  return (
    <div className={`backups-rule${open ? ' is-open' : ''}`} data-rule={place % 5}>
      <div className="backups-rule-head">
        <span className="backups-rule-mark" aria-hidden="true" />
        <strong>{ruleText(rule)}</strong>
        <span className="backups-tag">{keepText(rule)}</span>
        <span className="backups-tag">
          {held === items.length ? 'Everything' : `${held} of ${items.length}`}
        </span>
        <span className="backups-rule-tools">
          <button
            type="button"
            className="icon-button"
            aria-label="Delete rule"
            disabled={disabled}
            onClick={remove}
          >
            <Trash2 size={15} />
          </button>
          {open ? (
            <button
              type="button"
              className="icon-button"
              aria-label="Close rule"
              disabled={disabled}
              onClick={toggle}
            >
              <ChevronUp size={16} />
            </button>
          ) : (
            <button type="button" className="backups-edit" disabled={disabled} onClick={toggle}>
              <Pencil size={13} />
              Edit
            </button>
          )}
        </span>
      </div>
      {open && (
        <fieldset className="backups-rule-body" disabled={disabled}>
          <div className="backups-patterns" role="radiogroup" aria-label="Pattern">
            {PATTERNS.map((each) => (
              <button
                type="button"
                role="radio"
                key={each.pattern}
                className="backups-pattern"
                aria-checked={each.pattern === pattern}
                onClick={() => change(withPattern(rule, each.pattern))}
              >
                <Glyph pattern={each.pattern} />
                <span>{each.label}</span>
                {each.pattern === pattern && (
                  <span className="backups-check">
                    <Check size={12} />
                  </span>
                )}
              </button>
            ))}
          </div>
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
            {rule.kind === 'times' ? (
              <Times rule={rule} change={change} />
            ) : (
              <>
                <span className="backups-label">Every</span>
                <div className="backups-sentence">
                  <select
                    aria-label="Every"
                    value={rule.every ?? ''}
                    onChange={(e) => change({ ...rule, every: Number(e.target.value) })}
                  >
                    {EVERY.map((every) => (
                      <option key={every} value={every}>
                        {duration(every)}
                      </option>
                    ))}
                  </select>
                  <span>from</span>
                  <input
                    type="time"
                    aria-label="From"
                    required
                    value={rule.from ?? ''}
                    onChange={(e) => change({ ...rule, from: e.target.value })}
                  />
                  <span>to</span>
                  <input
                    type="time"
                    aria-label="To"
                    required
                    value={rule.to ?? ''}
                    onChange={(e) => change({ ...rule, to: e.target.value })}
                  />
                </div>
              </>
            )}
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

/** A day made once, a few times, or repeating during hours, drawn on a line. */
function Glyph({ pattern }: { pattern: Pattern }) {
  const pins = pattern === 'once' ? [50] : pattern === 'few' ? [25, 50, 75] : [];
  return (
    <svg className="backups-glyph" viewBox="0 0 100 24" aria-hidden="true">
      <rect x="4" y="10" width="92" height="4" rx="2" className="backups-glyph-track" />
      {pattern === 'repeat' && (
        <>
          <rect x="22" y="6" width="56" height="12" rx="4" className="backups-glyph-band" />
          {[28, 36, 44, 52, 60, 68].map((x) => (
            <rect
              key={x}
              x={x - 1}
              y="7"
              width="2"
              height="10"
              rx="1"
              className="backups-glyph-ink"
            />
          ))}
        </>
      )}
      {pins.map((x) => (
        <g key={x} className="backups-glyph-ink">
          <rect x={x - 1.5} y="5" width="3" height="14" rx="1.5" />
          <circle cx={x} cy="5" r="3.5" />
        </g>
      ))}
    </svg>
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

/** A rule's times: one, or a few, each changed, removed or added in place. */
function Times({ rule, change }: { rule: Draft; change: (rule: Draft) => void }) {
  const set = (index: number, value: string) =>
    change({ ...rule, times: rule.times.map((time, at) => (at === index ? value : time)) });
  const repeated = (time: string, index: number) => rule.times.indexOf(time) !== index;
  return (
    <>
      <span className="backups-label">At</span>
      <div className="backups-times">
        {rule.times.map((time, index) => (
          <span className="backups-time" key={index}>
            <input
              type="time"
              aria-label={`Time ${index + 1}`}
              required
              value={time}
              aria-invalid={(!!time && repeated(time, index)) || undefined}
              onChange={(e) => set(index, e.target.value)}
            />
            {rule.times.length > 1 && (
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove time ${index + 1}`}
                onClick={() =>
                  change({ ...rule, times: rule.times.filter((_, at) => at !== index) })
                }
              >
                <X size={14} />
              </button>
            )}
          </span>
        ))}
        {patternOf(rule) === 'few' && rule.times.length < MAX_TIMES && (
          <button
            type="button"
            className="text-button"
            onClick={() => change({ ...rule, times: [...rule.times, ''] })}
          >
            <Plus size={15} />
            Add a time
          </button>
        )}
      </div>
    </>
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
