import { useCallback } from 'react';
import { ApiError, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/data-reads.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { upload } from '../../../core/shell/frontend/runtime/transfer.js';
import type {
  CategoryInput,
  ConductBackup,
  ConductContext,
  ConductHistory,
  ConductImpact,
  ConductStepDown,
  ConductOverview,
  ConductSaved,
  ConductWriteup,
  WriteupInput,
} from './index.js';

// Conduct reads share prefetched data and word failures locally; writes invalidate their reads.

/** What its error codes say. Only its own screens make the calls that raise them. */
const errors: Readonly<Record<string, string>> = {
  conduct_person_not_found: 'This person is no longer in Driver Match. Choose someone else.',
  conduct_category_not_found: 'This category was retired. Choose another.',
  conduct_category_changed: 'This category changed in another session. Close it and open it again.',
  conduct_category_name_taken: 'Another category already has this name.',
  conduct_category_limit: 'A DSP can keep up to 100 categories in use.',
  conduct_writeup_not_found: 'This writeup was deleted.',
  conduct_writeup_changed: 'This writeup changed in another session. Open it again to see it.',
  conduct_writeup_signed: 'This writeup is signed, so it can no longer change.',
  conduct_copy_not_removed:
    'The backed-up copy couldn’t be removed, so nothing was deleted. Try again.',
  conduct_date_ahead: 'A writeup can’t be dated after today.',
  conduct_signed_date: 'The signed date must be between the writeup’s date and today.',
  conduct_suspension_required: 'Enter how long the suspension is.',
  conduct_section_required: 'Fill in every section.',
  conduct_copy_not_picture: 'Choose a photo of the signed paper.',
  conduct_request_conflict: 'This attempt already saved different changes. Open the writeup again.',
};
const api = wordedApi(errors);

function useConductRead<T>(url: string, refreshKey?: string) {
  const load = useCallback((signal: AbortSignal) => api<T>(url, undefined, signal), [url]);
  return useCachedData<T>(url, 0, refreshKey, load);
}

export const conductUrl = '/api/dsp/conduct';
/** Compact current standings and history totals. */
export const useConductOverview = () => useConductRead<ConductOverview>(conductUrl);

const writeupUrl = (id: string) => `${conductUrl}/writeups/${encodeURIComponent(id)}`;
/** The address of the photo of a writeup's signed paper. */
export const copyUrl = (id: string) => `${writeupUrl(id)}/copy`;

/** The writeup `id` whole, as it prints. */
export const useWriteup = (id: string, refreshKey?: string) =>
  useConductRead<ConductWriteup>(writeupUrl(id), refreshKey);

/** Acknowledged notice data replaces any older detail, even when its next read fails. */
async function saveWriteup(save: () => Promise<ConductSaved>) {
  const session = dataCache.session;
  const saved = await save();
  if (session === dataCache.session) dataCache.put(writeupUrl(saved.writeup.id), saved.writeup);
  return saved;
}
export const addWriteup = (input: WriteupInput) =>
  saveWriteup(() => api<ConductSaved>(`${conductUrl}/writeups`, input));
export const changeWriteup = (id: string, input: WriteupInput) =>
  saveWriteup(() => api<ConductSaved>(writeupUrl(id), input));
export async function signWriteup(
  id: string,
  signedOn: string,
  revision: number,
  request?: string,
) {
  try {
    return await saveWriteup(() =>
      api<ConductSaved>(`${writeupUrl(id)}/sign`, {
        signedOn,
        revision,
        ...(request ? { request } : {}),
      }),
    );
  } catch (cause) {
    dataCache.invalidate((url) => url.startsWith(conductUrl));
    throw cause;
  }
}
/** An irreversible action starts from an explicitly revalidated notice. */
export const readWriteup = (id: string) =>
  dataCache.read(
    writeupUrl(id),
    (signal) => api<ConductWriteup>(writeupUrl(id), undefined, signal),
    true,
  );
export const deleteWriteup = (id: string, revision: number) =>
  api<ConductOverview>(`${writeupUrl(id)}/delete`, { revision });
/** Keeps `photo` as the signed paper of the writeup `id` as it reads at `revision`. */
export async function keepCopy(id: string, revision: number, photo: Blob, request?: string) {
  try {
    return await saveWriteup(() =>
      upload<ConductSaved>(
        `${copyUrl(id)}?${new URLSearchParams({ revision: String(revision), ...(request ? { request } : {}) })}`,
        photo,
      ),
    );
  } catch (cause) {
    dataCache.invalidate((url) => url.startsWith(conductUrl));
    const message = cause instanceof ApiError ? errors[cause.code] : undefined;
    if (cause instanceof ApiError && message)
      throw new ApiError(cause.code, message, cause.status, cause.requestId);
    throw cause;
  }
}

export const backupUrl = `${conductUrl}/backup`;
/** How backing signed copies up is going, read again whenever `refreshKey` changes. */
export const useConductBackup = (refreshKey: string) =>
  useConductRead<ConductBackup>(backupUrl, refreshKey);

const categoryUrl = (id: string) => `${conductUrl}/categories/${encodeURIComponent(id)}`;
export const addCategory = (input: CategoryInput) =>
  api<ConductOverview>(`${conductUrl}/categories`, input);
export const changeCategory = (id: string, input: CategoryInput) =>
  api<ConductOverview>(categoryUrl(id), input);
export const retireCategory = (id: string) => api<ConductOverview>(`${categoryUrl(id)}/retire`, {});
export const restoreCategory = (id: string) =>
  api<ConductOverview>(`${categoryUrl(id)}/restore`, {});

export type HistoryFilters = { search: string; category: string; tier: string; status: string };
export const emptyFilters: HistoryFilters = { search: '', category: '', tier: '', status: '' };
export const useConductHistory = (filters: HistoryFilters, person = '', after = '') =>
  useConductRead<ConductHistory>(
    `${conductUrl}/writeups?${new URLSearchParams({ person, q: filters.search, category: filters.category, tier: filters.tier, status: filters.status, after })}`,
  );
export const useConductContext = (person: string, category: string, date: string, edit = '') =>
  useConductRead<ConductContext>(
    person && category
      ? `${conductUrl}/context?${new URLSearchParams({ person, category, date, edit })}`
      : '',
  );
export const useCategoryImpact = (
  id: string,
  revision: number,
  step: ConductStepDown | null,
  enabled: boolean,
) =>
  useConductRead<ConductImpact[]>(
    enabled
      ? `${categoryUrl(id)}/impact?${new URLSearchParams({ revision: String(revision), kind: step?.kind ?? 'never', ...(step ? { after: String(step.after), unit: step.unit } : {}) })}`
      : '',
  );
