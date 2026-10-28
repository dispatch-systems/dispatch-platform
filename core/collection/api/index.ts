import type { Narrow } from '../../foundation/api/narrow.js';
import type { ConnectionFeature } from '../../tenancy/api/index.js';
import type { Connection as GeneratedConnection } from './generated/Connection';
import type { ScheduleCollection } from './generated/ScheduleCollection';
import type { ScheduleRule } from './generated/ScheduleRule';

// Collection's API: jobs, the Collectors page's schedules, connections and the browser a member
// signs in through.
export type { PublicJob as Job } from './generated/PublicJob';
export type { JobMetrics } from './generated/JobMetrics';
export type { JobPhase } from './generated/JobPhase';
export type { JobOutcome } from './generated/JobOutcome';
export type { PageRead } from './generated/PageRead';
export type { PageReads } from './generated/PageReads';

export type { Collectors } from './generated/Collectors';
export type { LastRun } from './generated/LastRun';
export type { LiveReading } from './generated/LiveReading';
export type { RunCounts } from './generated/RunCounts';
export type { RuleKind } from './generated/RuleKind';
export type { ScheduleChoice } from './generated/ScheduleChoice';
export type { ScheduleOption } from './generated/ScheduleOption';
export type { ScheduleRule } from './generated/ScheduleRule';
export type { ScheduleRuns } from './generated/ScheduleRuns';
export type { ScheduledCollection } from './generated/ScheduledCollection';
/** A rule as a save or preview sends it: what's read from it, less what the server keeps. */
export type ScheduleRuleInput = Omit<ScheduleRule, 'nextRun' | 'lastError'>;
export type { ScheduleCollection };

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
