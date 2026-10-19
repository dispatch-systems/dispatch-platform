import { timezones } from '../../../../core/shell/frontend/lib/timezones.js';
import type { FieldKind, FieldProps } from './kind.js';

function TimezoneField({ id, value, change }: FieldProps<'timezone'>) {
  const zone = typeof value === 'string' ? value : '';
  return (
    <select id={id} value={zone} onChange={(event) => change(event.target.value)}>
      {[...(timezones.includes(zone) ? [] : [zone]), ...timezones].map((each) => (
        <option key={each}>{each}</option>
      ))}
    </select>
  );
}

/** A time zone, from those a DSP may keep its time in. */
export const timezone: FieldKind<'timezone'> = { Control: TimezoneField };
