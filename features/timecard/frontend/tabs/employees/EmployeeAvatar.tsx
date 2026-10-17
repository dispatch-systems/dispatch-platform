import { PersonAvatar } from '../../../../../core/shell/frontend/runtime/PersonAvatar.js';
import type { Employee } from '../../../api/index.js';

/** An employee's photo, from whoever the directory finds they are, or their initials. */
export function EmployeeAvatar({ employee }: { employee: Employee }) {
  return (
    <PersonAvatar
      name={employee.name}
      code={employee.person}
      photo={employee.photo}
      className="employee-avatar"
    />
  );
}
