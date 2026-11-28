import type { DemoStatus } from '../../api/index.js';

/** Where the owner may stand with a request, in the order it moves through them. */
export const statuses: readonly DemoStatus[] = ['new', 'contacted', 'demo', 'signed', 'declined'];
export const statusLabels: Record<DemoStatus, string> = {
  new: 'New',
  contacted: 'Contacted',
  demo: 'Demo booked',
  signed: 'Signed up',
  declined: 'Not a fit',
};
