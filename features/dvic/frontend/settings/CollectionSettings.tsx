import { ScheduleSettings } from '../../../../core/collection/frontend/schedules.js';
import { schedules } from '../../api/client.js';

/** DVIC's schedules, which collect the latest weeks' reports each evening by default. */
export function CollectionSettings({ onClose }: { onClose: () => void }) {
  return (
    <ScheduleSettings
      endpoint={schedules}
      collection="dvic"
      defaults={{ name: 'Daily DVIC', localTime: '18:00' }}
      onClose={onClose}
    />
  );
}
