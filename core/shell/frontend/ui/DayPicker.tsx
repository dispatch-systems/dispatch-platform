import { WEEKDAYS } from '../lib/calendar.js';

/**
 * The days of the week, Monday first, a button each, pressed while chosen: `days` holds the
 * chosen ones by their place in the week, Monday 0, in order.
 */
export function DayPicker({
  days,
  invalid,
  change,
}: {
  days: readonly number[];
  /** Whether what's chosen can't be kept, as none. */
  invalid?: boolean;
  change: (days: number[]) => void;
}) {
  return (
    <div className="day-picker">
      {WEEKDAYS.map((day, index) => {
        const on = days.includes(index);
        return (
          <button
            type="button"
            key={day}
            aria-pressed={on}
            aria-invalid={invalid || undefined}
            onClick={() =>
              change(
                on ? days.filter((each) => each !== index) : [...days, index].sort((a, b) => a - b),
              )
            }
          >
            {day}
          </button>
        );
      })}
    </div>
  );
}
