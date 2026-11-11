import {
  SchedulePatterns,
  ScheduleTimes,
  ScheduleRuleTools,
} from '../../../shell/frontend/ui/ScheduleEditor.js';
import type { ScheduleChoice } from '../../api/index.js';
import { DayPicker } from '../../../shell/frontend/ui/index.js';
import { Mark } from './glyph.js';
import { PATTERNS, patternOf, ruleText, type Draft } from './rules.js';

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
    <div className={`collectors-rule schedule-rule${open ? ' is-open' : ''}`} data-rule={place % 5}>
      <div className="schedule-rule-head">
        <span className="collectors-rule-mark">
          <Mark pattern={pattern} />
        </span>
        <strong>{ruleText(rule)}</strong>
        {choice && chosen && <span className="collectors-tag">{chosen.label}</span>}
        {!open && <span className="collectors-kind">{kind}</span>}
        {issue && <span className="collectors-issue">{issue}</span>}
        <ScheduleRuleTools open={open} disabled={disabled} toggle={toggle} remove={remove} />
      </div>
      {open && (
        <fieldset className="schedule-rule-body" disabled={disabled}>
          <SchedulePatterns rule={rule} change={change} />
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
            <DayPicker
              days={rule.days}
              invalid={!rule.days.length}
              change={(days) => change({ ...rule, days })}
            />
            <ScheduleTimes rule={rule} change={change} />
          </div>
        </fieldset>
      )}
    </div>
  );
}
