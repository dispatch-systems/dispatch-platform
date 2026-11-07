import { api } from '../../shell/frontend/runtime/api.js';
import { useData } from '../../shell/frontend/runtime/data-reads.js';
import type { BackupRuleInput, BackupRuns, DspBackups } from './index.js';

// A DSP's Backups tab, as the platform owner's dashboard calls it.

const backups = (dspId: string) => `/api/platform/dsps/${dspId}/backups`;
/** The DSP's backups, read again every `poll` milliseconds while one is being taken. */
export const useDspBackups = (dspId: string, poll = 0) => useData<DspBackups>(backups(dspId), poll);
/** Saves the DSP's backup rules together, as at the revision they were read at. */
export const saveBackupRules = (dspId: string, rules: BackupRuleInput[], revision: number) =>
  api<DspBackups>(`${backups(dspId)}/schedule`, { rules, revision });
/** Turns the DSP's backup schedule on or off. */
export const switchBackups = (dspId: string, enabled: boolean) =>
  api<DspBackups>(`${backups(dspId)}/switch`, { enabled });
/** When unsaved rules would run next. */
export const previewBackups = (dspId: string, rules: BackupRuleInput[], signal: AbortSignal) =>
  api<BackupRuns>(`${backups(dspId)}/preview`, { rules }, signal);
/** Takes a backup of the DSP now. */
export const backUpNow = (dspId: string) => api<DspBackups>(`${backups(dspId)}/now`, {});
/** Where one of the DSP's backups is saved from, as a zip. */
export const backupDownload = (dspId: string, backupId: string) =>
  `${backups(dspId)}/${backupId}/download`;
