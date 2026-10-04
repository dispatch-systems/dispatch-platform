import type { Narrow } from '../../foundation/api/narrow.js';
import type { ConnectionFeature } from '../../tenancy/api/index.js';
import type { Connection as GeneratedConnection } from './generated/Connection';

// Collection's API: jobs, schedules, connections and the browser a member signs in through.
export type { PublicJob as Job } from './generated/PublicJob';
export type { JobMetrics } from './generated/JobMetrics';
export type { JobPhase } from './generated/JobPhase';
export type { JobOutcome } from './generated/JobOutcome';
export type { PageRead } from './generated/PageRead';
export type { PageReads } from './generated/PageReads';

export interface ScheduleInput {
  name: string;
  collection: 'paycom' | 'meal_break' | 'both' | 'scorecard' | 'routes' | 'dvic';
  cadence: 'interval' | 'daily';
  intervalMinutes: number | null;
  localTime: string;
  enabled: boolean;
}
export type { CollectionSchedule } from './generated/CollectionSchedule';
export type { CollectionSchedules } from './generated/CollectionSchedules';
export type { SchedulePreview } from './generated/SchedulePreview';

export type Connection = Narrow<GeneratedConnection, { provider: ConnectionFeature }>;

export type { CollectionUpdates } from './generated/CollectionUpdates';
export type { CollectionChange } from './generated/CollectionChange';

export type BrowserInput =
  | { kind: 'click'; x: number; y: number }
  | { kind: 'pointer'; phase: 'down' | 'move' | 'up'; x: number; y: number; pressed: boolean }
  | { kind: 'scroll'; x: number; y: number; deltaX: number; deltaY: number }
  | { kind: 'type'; text: string }
  | {
      kind: 'key';
      key:
        | 'Enter'
        | 'Tab'
        | 'Backspace'
        | 'Delete'
        | 'Escape'
        | 'ArrowDown'
        | 'ArrowUp'
        | 'ArrowLeft'
        | 'ArrowRight'
        | 'Home'
        | 'End'
        | 'PageUp'
        | 'PageDown';
      shift?: boolean;
    };
export interface BrowserFrame {
  image: string;
  sessionId: string;
}
