export type { Backup } from './generated/Backup';
export type { BackupItem } from './generated/BackupItem';
export type { BackupRule } from './generated/BackupRule';
export type { BackupRuns } from './generated/BackupRuns';
export type { BackupStatus } from './generated/BackupStatus';
export type { DspBackups } from './generated/DspBackups';
export type { KeepUnit } from './generated/KeepUnit';
import type { BackupRule } from './generated/BackupRule';

/** A backup rule as it's sent: without when it runs next, which the server works out. */
export type BackupRuleInput = Omit<BackupRule, 'nextRun'>;
