import { ScheduleSettings } from '../../../../core/collection/frontend/schedules.js';
import type { FieldKind, FieldProps } from './kind.js';

/**
 * Core's schedule editor, in the page, under its own heading unless it's all its section holds:
 * the page asks it before moving to another of its pages.
 */
function Schedules({ field, alone }: FieldProps<'schedules'>) {
  return (
    <div className="settings-field settings-wide settings-schedules">
      {!alone && <h3>{field.label}</h3>}
      <ScheduleSettings
        endpoint={field.path}
        collection={field.collection}
        defaults={{ name: field.starts.name, localTime: field.starts.localTime }}
      />
    </div>
  );
}

/** A collection's schedules, which save themselves rather than with the section. */
export const schedules: FieldKind<'schedules'> = {
  Control: Schedules,
  labelled: () => true,
  apart: true,
};
