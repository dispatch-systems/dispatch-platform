import type { DemoStatus } from '../../api/index.js';
import { statusLabels } from './statuses.js';

/** Where a request stands, as a dot of the status's colour beside its name. */
export const Status = ({ status }: { status: DemoStatus }) => (
  <span className={`status-indicator client-dot ${status}`}>
    <i />
    {statusLabels[status]}
  </span>
);
