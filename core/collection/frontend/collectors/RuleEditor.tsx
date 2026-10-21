import { Check, ChevronUp, Pencil, Plus, Trash2, X } from 'lucide-react';
import type { ScheduleChoice } from '../../api/index.js';
import { Glyph, Mark } from './glyph.js';
import {
  DAYS,
  EVERY,
  MAX_TIMES,
  PATTERNS,
  duration,
  patternOf,
  ruleText,
  withPattern,
  type Draft,
} from './rules.js';

// One rule of a collection's schedule: its words, and opened, the pattern it's made from and
// that pattern's fields, which change it as they're set.

export function RuleEditor({
  rule,
  place,
  open,
  choice,
  issue,
  disabled,
  change,
  toggle,
  remove,
}: {
  rule: Draft;
  /** Its place among the rules, which colours it as the week draws it. */
  place: number;
  open: boolean;
  choice: ScheduleChoice | null;
  /** Why its run waits, in words, if it does. */
  issue?: string;
  disabled: boolean;
  change: (rule: Draft) => void;
  toggle: () => void;
  remove: () => void;
}) {
  const pattern = patternOf(rule);
  const chosen = choice?.options.find((option) => option.collection === rule.collects);
  const kind = PATTERNS.find((each) => each.pattern === pattern)!.label;
  return (
    <div className={`collectors-rule${open ? ' is-open' : ''}`} data-rule={place % 5}>
      <div className="collectors-rule-head">
        <span className="collectors-rule-mark">
          <Mark pattern={pattern} />
        </span>
        <strong>{ruleText(rule)}</strong>
        {choice && chosen && <span className="collectors-tag">{chosen.label}</span>}
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
            {choice && (
              <>
                <span className="collectors-label">{choice.label}</span>
                <div className="settings-segments" role="radiogroup" aria-label={choice.label}>
                  {choice.options.map((option) => (
                    <label key={option.collection}>
                      <input
                        type="radio"
                        name={`${rule.key}-collects`}
                        checked={option.collection === rule.collects}
                        onChange={() => change({ ...rule, collects: option.collection })}
                      />
                      {option.label}
                    </label>
                  ))}
                </div>
              </>
            )}
            <span className="collectors-label">Days</span>
            <div className="collectors-days">
              {DAYS.map((day, index) => {
                const on = rule.days.includes(index);
                return (
                  <button
                    type="button"
                    key={day}
                    aria-pressed={on}
                    aria-invalid={!rule.days.length || undefined}
                    onClick={() =>
                      change({
                        ...rule,
                        days: on
                          ? rule.days.filter((each) => each !== index)
                          : [...rule.days, index].sort(),
                      })
                    }
                  >
                    {day}
                  </button>
                );
              })}
            </div>
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
          </div>
        </fieldset>
      )}
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
