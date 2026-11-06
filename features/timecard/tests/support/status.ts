import type { Job } from '../../../../core/collection/api/index.js';

export function syncJob(status: Job['status'], kind: Job['kind']): Job {
  return {
    id: `${kind}-job`,
    dspId: 'fixture-dsp',
    dspName: 'Fixture DSP',
    environment: 'preview',
    kind,
    status,
    progress: 0,
    message: '',
    attempt: 1,
    maxAttempts: 3,
    availableAt: '2026-09-15T00:00:00Z',
    createdAt: '2026-09-15T00:00:00Z',
    startedAt: null,
    completedAt: null,
    error: null,
    release: '',
    actorId: null,
    metrics: [],
  };
}
