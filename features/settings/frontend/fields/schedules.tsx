import { useRef } from 'react';
import { ScheduleSettings, type Leave } from '../../../../core/collection/frontend/schedules.js';
import { useLeaveGuard } from '../leave.js';
import type { FieldKind, FieldProps } from './kind.js';

/** Core's schedule editor, in the page: the page asks it before moving to another of its pages. */
function Schedules({ field }: FieldProps<'schedules'>) {
  const leave = useRef<Leave | null>(null);
  useLeaveGuard(leave);
  return (
    <div className="settings-field settings-wide settings-schedules">
      <h3>{field.label}</h3>
      <ScheduleSettings
        inline
        endpoint={field.path}
        collection={field.collection}
        defaults={{ name: field.starts.name, localTime: field.starts.localTime }}
        leaveRef={leave}
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
