import { PersonAvatar } from '../../../../../core/shell/frontend/runtime/PersonAvatar.js';
import type { Employee } from '../../../api/index.js';
import { employeesPath } from '../../../api/urls.js';

/** An employee's photo, from whoever Driver Match finds they are, or their initials. */
export function EmployeeAvatar({ employee }: { employee: Employee }) {
  const photo =
    employee.photo &&
    `${employeesPath}/${encodeURIComponent(employee.code)}/photo?v=${employee.photo.slice(0, 16)}`;
  return <PersonAvatar name={employee.name} photo={photo || undefined} className="employee-avatar" />;
}
