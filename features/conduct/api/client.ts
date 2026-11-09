import { wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import { useData } from '../../../core/shell/frontend/runtime/data-reads.js';
import { upload } from '../../../core/shell/frontend/runtime/transfer.js';
import type {
  CategoryInput,
  ConductOverview,
  ConductSaved,
  ConductWriteup,
  WriteupInput,
} from './index.js';

// Conduct's endpoints, as its screens call them. Each write answers with the page's data as
// it is after it, so the page shows the change at once.

/** What its error codes say. Only its own screens make the calls that raise them. */
const api = wordedApi({
  conduct_person_not_found: 'This person is no longer in Driver Match. Choose someone else.',
  conduct_category_not_found: 'This category was retired. Choose another.',
  conduct_category_changed: 'This category changed in another session. Close it and open it again.',
  conduct_category_name_taken: 'Another category already has this name.',
  conduct_category_limit: 'A DSP can keep up to 100 categories in use.',
  conduct_writeup_not_found: 'This writeup was deleted.',
  conduct_writeup_changed: 'This writeup changed in another session. Open it again to see it.',
  conduct_writeup_signed: 'This writeup is signed, so it can no longer change.',
  conduct_date_ahead: 'A writeup can’t be dated after today.',
  conduct_signed_date: 'The signed date must be between the writeup’s date and today.',
  conduct_suspension_required: 'Enter how long the suspension is.',
  conduct_section_required: 'Fill in every section.',
  conduct_copy_not_picture: 'Choose a photo of the signed paper.',
});

export const conductUrl = '/api/dsp/conduct';
/** Everything the page draws. */
export const useConductOverview = () => useData<ConductOverview>(conductUrl);

const writeupUrl = (id: string) => `${conductUrl}/writeups/${encodeURIComponent(id)}`;
/** The address of the photo of a writeup's signed paper. */
export const copyUrl = (id: string) => `${writeupUrl(id)}/copy`;

/** The writeup `id` whole, as it prints. */
export const useWriteup = (id: string, refreshKey?: string) =>
  useData<ConductWriteup>(writeupUrl(id), 0, refreshKey);

export const addWriteup = (input: WriteupInput) =>
  api<ConductSaved>(`${conductUrl}/writeups`, input);
export const changeWriteup = (id: string, input: WriteupInput) =>
  api<ConductSaved>(writeupUrl(id), input);
export const signWriteup = (id: string, signedOn: string, revision: number) =>
  api<ConductSaved>(`${writeupUrl(id)}/sign`, { signedOn, revision });
export const deleteWriteup = (id: string, revision: number) =>
  api<ConductOverview>(`${writeupUrl(id)}/delete`, { revision });
export const keepCopy = (id: string, photo: Blob) => upload<ConductSaved>(copyUrl(id), photo);

const categoryUrl = (id: string) => `${conductUrl}/categories/${encodeURIComponent(id)}`;
export const addCategory = (input: CategoryInput) =>
  api<ConductOverview>(`${conductUrl}/categories`, input);
export const changeCategory = (id: string, input: CategoryInput) =>
  api<ConductOverview>(categoryUrl(id), input);
export const retireCategory = (id: string) => api<ConductOverview>(`${categoryUrl(id)}/retire`, {});
export const restoreCategory = (id: string) =>
  api<ConductOverview>(`${categoryUrl(id)}/restore`, {});
