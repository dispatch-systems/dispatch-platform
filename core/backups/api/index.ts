export type { Backup } from './generated/Backup';
export type { BackupDay } from './generated/BackupDay';
export type { BackupItem } from './generated/BackupItem';
export type { BackupOn } from './generated/BackupOn';
export type { BackupRule } from './generated/BackupRule';
export type { BackupRuns } from './generated/BackupRuns';
export type { BackupStatus } from './generated/BackupStatus';
export type { BackupTiming } from './generated/BackupTiming';
export type { DspBackups } from './generated/DspBackups';
export type { KeepFor } from './generated/KeepFor';
export type { KeepUnit } from './generated/KeepUnit';
import type { BackupRule } from './generated/BackupRule';

/** A backup rule as it's sent: without when it runs next, which the server works out. */
export type BackupRuleInput = Omit<BackupRule, 'nextRun'>;
