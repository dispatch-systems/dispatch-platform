import { api, useData } from '../../../core/shell/frontend/runtime/api.js';
import type {
  UniformAdjustment,
  UniformHistory,
  UniformInput,
  UniformInventory,
  UniformUpdates,
} from './index.js';

// Uniform Inventory's endpoints, as its page calls them.

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
