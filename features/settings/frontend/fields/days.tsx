import type { SettingsValue } from '../../../../core/settings/api/index.js';
import { DayPicker } from '../../../../core/shell/frontend/ui/index.js';
import type { FieldKind, FieldProps } from './kind.js';

// The days as the server keeps them, Monday first.
const KEPT = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];

/** The chosen days, by their place in the week, Monday 0. */
const places = (value: SettingsValue) =>
  KEPT.flatMap((day, place) => (Array.isArray(value) && value.includes(day) ? [place] : []));

function Days({ field, id, value, change }: FieldProps<'days'>) {
  return (
    <div className="settings-field" role="group" aria-labelledby={`${id}-label`}>
      <span id={`${id}-label`}>{field.label}</span>
      <DayPicker
        days={places(value)}
        change={(days) => change(days.map((place) => KEPT[place]!))}
      />
    </div>
  );
}

/** Any of the days of the week. */
export const days: FieldKind<'days'> = { Control: Days, labelled: () => true };
