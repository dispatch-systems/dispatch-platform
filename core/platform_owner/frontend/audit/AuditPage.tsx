import { AuditLog } from './AuditLog.js';
import { Header } from '../../../shell/frontend/ui/index.js';

export function AuditPage() {
  return (
    <>
      <Header title="Audit log" />
      <AuditLog />
    </>
  );
}
