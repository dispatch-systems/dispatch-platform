import type { Narrow } from '../../foundation/api/narrow.js';
import type { ConnectionFeature } from '../../tenancy/api/index.js';
import type { Connection as GeneratedConnection } from '../../../shared/contracts/generated/Connection';

// Collection's API: jobs, schedules, connections and the browser a member signs in through.
export type { PublicJob as Job } from '../../../shared/contracts/generated/PublicJob';
export type { JobMetrics } from '../../../shared/contracts/generated/JobMetrics';
export type { JobPhase } from '../../../shared/contracts/generated/JobPhase';
export type { JobOutcome } from '../../../shared/contracts/generated/JobOutcome';
export type { PageRead } from '../../../shared/contracts/generated/PageRead';
export type { PageReads } from '../../../shared/contracts/generated/PageReads';

export interface ScheduleInput {
  name: string;
  collection: 'paycom' | 'meal_break' | 'both' | 'scorecard' | 'routes' | 'dvic';
  cadence: 'interval' | 'daily';
  intervalMinutes: number | null;
  localTime: string;
  enabled: boolean;
}
export type { CollectionSchedule } from '../../../shared/contracts/generated/CollectionSchedule';
export type { CollectionSchedules } from '../../../shared/contracts/generated/CollectionSchedules';
export type { SchedulePreview } from '../../../shared/contracts/generated/SchedulePreview';

export type Connection = Narrow<GeneratedConnection, { provider: ConnectionFeature }>;

export type { CollectionUpdates } from '../../../shared/contracts/generated/CollectionUpdates';
export type { CollectionChange } from '../../../shared/contracts/generated/CollectionChange';

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
