import { useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type {
  UniformAdjustment,
  UniformHistory,
  UniformInput,
  UniformInventory,
  UniformUpdates,
} from './index.js';

// Uniform Inventory's endpoints, as its page calls them.

/** What its error codes say. Only its page makes the calls that raise them, so they load with it. */
const api = wordedApi({
  uniform_changed:
    'This uniform changed in another session. Close and reopen the editor before saving.',
  uniform_not_found: 'This uniform was removed. Refresh the inventory.',
  uniform_size_not_found: 'This size was removed or changed. Refresh the inventory.',
  uniform_name_taken: 'Another uniform already uses this name.',
  uniform_size_duplicate: 'Each fit can only have one entry for a size.',
  invalid_uniform_size: 'Size names must contain 1 to 24 characters.',
  uniform_size_in_stock: 'Remove the remaining stock before removing a size.',
  uniform_in_stock: 'Remove the remaining stock before archiving a uniform.',
  uniform_out_of_stock:
    'Another adjustment used the remaining stock. The current count has been refreshed.',
  uniform_quantity_limit: 'This size has reached the inventory limit.',
  uniform_inventory_initialized: 'Inventory was already set up by another user. Refresh to see it.',
  uniform_limit: 'You can create up to 200 uniforms per DSP.',
  uniform_size_limit: 'A uniform can have up to 150 size and fit combinations.',
  uniform_request_conflict: 'This inventory request does not match its original adjustment.',
});

// Each active inventory page holds one long poll. Unchanged quantities send no rows.
export const getUniformUpdates = (after: number | undefined, signal: AbortSignal) =>
  api<UniformUpdates>(
    `/api/dsp/uniforms/updates${after === undefined ? '' : `?after=${after}`}`,
    undefined,
    signal,
  );
export const initializeUniforms = (starter: boolean) =>
  api<UniformInventory>('/api/dsp/uniforms/initialize', { starter });
export const saveUniform = (id: string | undefined, input: UniformInput) =>
  api<UniformInventory>(`/api/dsp/uniforms${id ? `/${id}` : ''}`, input);
export const archiveUniform = (id: string, revision: number) =>
  api<UniformInventory>(`/api/dsp/uniforms/${id}/archive`, { revision });
export const adjustUniform = (id: string, delta: 1 | -1, requestId: string, signal?: AbortSignal) =>
  api<UniformAdjustment>(`/api/dsp/uniforms/stock/${id}`, { delta, requestId }, signal);
export const useUniformHistory = (before: number | null, revision: number) =>
  useData<UniformHistory>(
    `/api/dsp/uniforms/history${before === null ? '' : `?before=${before}`}`,
    0,
    String(revision),
  );
