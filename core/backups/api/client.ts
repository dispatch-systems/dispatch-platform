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
/** Saves one of the DSP's backups as a zip, once the owner has confirmed it's them recently: the
 * dashboard asks them to first, as a download holds the DSP's saved logins. */
export async function downloadBackup(dspId: string, backupId: string) {
  const url = `${backups(dspId)}/${backupId}/download`;
  const allowed = await api<{ ticket: string; until: number }>(url, {});
  const link = document.createElement('a');
  link.href = `${url}?${new URLSearchParams({ ticket: allowed.ticket, until: String(allowed.until) })}`;
  link.download = '';
  link.click();
}
