import type { AuditPhrases, AuditWording } from '../../../core/shell/frontend/runtime/slots.js';

// How Routes' events read in the platform owner's audit log.

const phrases: AuditPhrases = {
  'routes.collection_requested': (e, { strong, day }) => [
    'started a routes collection',
    ...(e.detail && e.detail !== 'routes' ? [' for ', strong(day(e.detail))] : []),
  ],
  'routes.retention_changed': (e, { strong }) =>
    e.detail === 'forever'
      ? ['set route data to be kept ', strong('indefinitely')]
      : ['set route data to be kept for ', strong(`${e.detail} days`)],
  'routes.data_expired': (e, { strong, day }) => [
    'deleted route data from before ',
    strong(day(e.detail)),
    ', as the retention setting asks',
  ],
};

export const wording: AuditWording = {
  phrases,
  spoken: ['routes.collection_requested', 'routes.retention_changed', 'routes.data_expired'],
};
