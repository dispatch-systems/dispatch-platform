export interface ScheduleInput {
  name: string;
  collection: 'paycom' | 'meal_break' | 'both' | 'scorecard' | 'routes';
  cadence: 'interval' | 'daily';
  intervalMinutes: number | null;
  localTime: string;
  enabled: boolean;
}
export type { CollectionSchedule } from './generated/CollectionSchedule';
export type { CollectionSchedules } from './generated/CollectionSchedules';
export type { SchedulePreview } from './generated/SchedulePreview';
