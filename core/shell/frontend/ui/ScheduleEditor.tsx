import { Check, ChevronUp, Pencil, Plus, Trash2, X } from 'lucide-react';
import {
  EVERY,
  MAX_TIMES,
  PATTERNS,
  duration,
  patternOf,
  withPattern,
  type Pattern,
  type ScheduleTiming,
} from '../lib/schedule-rules.js';
import './schedule.css';

/** The three ways to time a day's runs, keeping the owner's other fields. */
export function SchedulePatterns<T extends ScheduleTiming>({
  rule,
  change,
}: {
  rule: T;
  change: (rule: T) => void;
}) {
  const pattern = patternOf(rule);
  return (
    <div className="schedule-patterns" role="radiogroup" aria-label="Pattern">
      {PATTERNS.map((each) => (
        <button
          type="button"
          role="radio"
          key={each.pattern}
          className="schedule-pattern"
          aria-checked={each.pattern === pattern}
          onClick={() => change(withPattern(rule, each.pattern))}
        >
          <Glyph pattern={each.pattern} />
          <span>{each.label}</span>
          {each.pattern === pattern && (
            <span className="schedule-check">
              <Check size={12} />
            </span>
          )}
        </button>
      ))}
    </div>
  );
}

/** Set times, or an interval between two times. */
export function ScheduleTimes<T extends ScheduleTiming>({
  rule,
  change,
}: {
  rule: T;
  change: (rule: T) => void;
}) {
  return rule.kind === 'times' ? (
    <Times rule={rule} change={change} />
  ) : (
    <>
      <span className="schedule-label">Every</span>
      <div className="schedule-sentence">
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
  );
}

/** A day made once, a few times, or repeating during hours, drawn on a line. */
function Glyph({ pattern }: { pattern: Pattern }) {
  const pins = pattern === 'once' ? [50] : pattern === 'few' ? [25, 50, 75] : [];
  return (
    <svg className="schedule-glyph" viewBox="0 0 100 24" aria-hidden="true">
      <rect x="4" y="10" width="92" height="4" rx="2" className="schedule-glyph-track" />
      {pattern === 'repeat' && (
        <>
          <rect x="22" y="6" width="56" height="12" rx="4" className="schedule-glyph-band" />
          {[28, 36, 44, 52, 60, 68].map((x) => (
            <rect
              key={x}
              x={x - 1}
              y="7"
              width="2"
              height="10"
              rx="1"
              className="schedule-glyph-ink"
            />
          ))}
        </>
      )}
      {pins.map((x) => (
        <g key={x} className="schedule-glyph-ink">
          <rect x={x - 1.5} y="5" width="3" height="14" rx="1.5" />
          <circle cx={x} cy="5" r="3.5" />
        </g>
      ))}
    </svg>
  );
}

/** A rule's times: one, or a few, each changed, removed or added in place. */
function Times<T extends ScheduleTiming>({ rule, change }: { rule: T; change: (rule: T) => void }) {
  const set = (index: number, value: string) =>
    change({ ...rule, times: rule.times.map((time, at) => (at === index ? value : time)) });
  const repeated = (time: string, index: number) => rule.times.indexOf(time) !== index;
  return (
    <>
      <span className="schedule-label">At</span>
      <div className="schedule-times">
        {rule.times.map((time, index) => (
          <span className="schedule-time" key={index}>
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

/** The edit, close and delete controls shared by schedule cards. */
export function ScheduleRuleTools({
  open,
  disabled,
  toggle,
  remove,
}: {
  open: boolean;
  disabled: boolean;
  toggle: () => void;
  remove: () => void;
}) {
  return (
    <span className="schedule-rule-tools">
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
        <button type="button" className="schedule-edit" disabled={disabled} onClick={toggle}>
          <Pencil size={13} />
          Edit
        </button>
      )}
    </span>
  );
}
