import { api, useCachedData } from '../../shell/frontend/runtime/api.js';
import { dataCache } from '../../shell/frontend/runtime/data-cache.js';
import { settingsRead } from '../../shell/frontend/runtime/slots.js';
import type { SettingsAnswer, SettingsSave, SettingsSaved } from './index.js';

// Settings' endpoints, as a DSP's Settings page calls them. Each is the open DSP's: the server
// reads the DSP from the view the API client sends, and the cache holding what they read is
// emptied whenever that view changes, as it does for another DSP or another role viewed as.

/** What the member's Settings shows: its pages, and the sections on them. */
export const useSettings = () => useCachedData<SettingsAnswer>(settingsRead);
/** The member's Settings, once read, while the cache still holds it. */
export const readSettings = () => dataCache.peek(settingsRead).data as SettingsAnswer | undefined;
/** Reads the member's Settings, for a page about to draw it. */
export const getSettings = (signal?: AbortSignal) =>
  api<SettingsAnswer>(settingsRead, undefined, signal);

/**
 * Saves the values of a section that changed. Once saved, the section as saved takes its place
 * in what Settings read, so the page shows it at once, unless another view opened meanwhile; a
 * save that changed the DSP itself, as its name, opens its view again; a warning saves nothing.
 */
export async function saveSettings(section: string, save: SettingsSave) {
  const session = dataCache.session;
  const answer = await api<SettingsSaved>(`${settingsRead}/${encodeURIComponent(section)}`, save);
  const current = readSettings();
  if ('section' in answer && current && dataCache.session === session)
    dataCache.put(settingsRead, {
      ...current,
      sections: current.sections.map((each) => (each.id === section ? answer.section : each)),
    });
  if ('section' in answer && answer.reopen)
    window.dispatchEvent(new Event('dispatch-view-expired'));
  return answer;
}
