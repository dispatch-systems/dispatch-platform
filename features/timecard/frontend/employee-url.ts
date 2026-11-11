import { employeesPath } from '../api/urls.js';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';

// The directory read and its prefetch use the same cache key.
export const employeesUrl = (query: string, status: string, descending: boolean, page = 0) =>
  `${employeesPath}?q=${encodeURIComponent(query)}&status=${status}&limit=${performancePolicy.employeePageSize}&offset=${page * performancePolicy.employeePageSize}&direction=${descending ? 'desc' : 'asc'}`;
