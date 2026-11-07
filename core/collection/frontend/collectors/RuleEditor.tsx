import { Check, ChevronUp, Pencil, Plus, Trash2, X } from 'lucide-react';
import type { ReactNode } from 'react';
import { DayPicker } from '../../../shell/frontend/ui/index.js';
import { Glyph, Mark } from './glyph.js';
import {
  EVERY,
  MAX_TIMES,
  PATTERNS,
  duration,
  patternOf,
  ruleText,
  withPattern,
  type Editing,
} from './rules.js';

// One rule of a schedule: its words, and opened, the pattern it's made from and that pattern's
// fields, which change it as they're set. A collection's rules and every schedule made the same
// way, a DSP's backups among them, are edited here.

export function RuleEditor<R extends Editing>({
  rule,
  place,
  open,
  tags,
  lead,
  more,
  issue,
  disabled,
  change,
  toggle,
  remove,
}: {
  rule: R;
  /** Its place among the rules, which colours it as the week draws it. */
  place: number;
  open: boolean;
  /** What its words are followed by, as a collection's rule names what it reads. */
  tags?: ReactNode;
  /** Its schedule's own fields before its days, as a collection's what it reads. */
  lead?: ReactNode;
  /** Its schedule's own fields after when it runs, as a backup's how long it's kept. */
  more?: ReactNode;
  /** Why its run waits, in words, if it does. */
  issue?: string;
  disabled: boolean;
  change: (rule: R) => void;
  toggle: () => void;
  remove: () => void;
}) {
  const pattern = patternOf(rule);
  const kind = PATTERNS.find((each) => each.pattern === pattern)!.label;
  return (
    <div className={`collectors-rule${open ? ' is-open' : ''}`} data-rule={place % 5}>
      <div className="collectors-rule-head">
        <span className="collectors-rule-mark">
          <Mark pattern={pattern} />
        </span>
        <strong>{ruleText(rule)}</strong>
        {tags}
        {!open && <span className="collectors-kind">{kind}</span>}
        {issue && <span className="collectors-issue">{issue}</span>}
        <span className="collectors-rule-tools">
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
            <button type="button" className="collectors-edit" disabled={disabled} onClick={toggle}>
              <Pencil size={13} />
              Edit
            </button>
          )}
        </span>
      </div>
      {open && (
        <fieldset className="collectors-rule-body" disabled={disabled}>
          <div className="collectors-patterns" role="radiogroup" aria-label="Pattern">
            {PATTERNS.map((each) => (
              <button
                type="button"
                role="radio"
                key={each.pattern}
                className="collectors-pattern"
                aria-checked={each.pattern === pattern}
                onClick={() => change(withPattern(rule, each.pattern))}
              >
                <span className="collectors-art">
                  <Glyph pattern={each.pattern} />
                </span>
                <span>{each.label}</span>
                {each.pattern === pattern && (
                  <span className="collectors-check">
                    <Check size={12} />
                  </span>
                )}
              </button>
            ))}
          </div>
          <div className={`collectors-fields at-${pattern}`}>
            {lead}
            <span className="collectors-label">Days</span>
            <DayPicker
              days={rule.days}
              invalid={!rule.days.length}
              change={(days) => change({ ...rule, days })}
            />
            {rule.kind === 'times' ? (
              <Times rule={rule} change={change} />
            ) : (
              <>
                <span className="collectors-label">Every</span>
                <div className="collectors-sentence">
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
            {more}
          </div>
        </fieldset>
      )}
    </div>
  );
}

/** A rule's times: one, or a few, each changed, removed or added in place. */
function Times<R extends Editing>({ rule, change }: { rule: R; change: (rule: R) => void }) {
  const set = (index: number, value: string) =>
    change({ ...rule, times: rule.times.map((time, at) => (at === index ? value : time)) });
  const repeated = (time: string, index: number) => rule.times.indexOf(time) !== index;
  return (
    <>
      <span className="collectors-label">At</span>
      <div className="collectors-times">
        {rule.times.map((time, index) => (
          <span className="collectors-time" key={index}>
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
