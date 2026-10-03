import type { DspView } from '../../../core/accounts/api/index.js';
import type { EmployeesResponse } from '../api/index.js';
import { readUpdateState } from '../../../core/shell/frontend/runtime/browser-update.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { dailyTimecardsUrl, employeeTimecardUrl, mealComparisonUrl } from '../api/client.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { dspHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { admitted, warm } from '../../../core/shell/frontend/runtime/route-prefetch.js';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { selectedPaycomDate } from './paycom-date.js';

// What the Timecard reads first, so that its selected tab's data starts with its code.

function timecardUrls(tab: string, date: string, view: DspView): string[] {
  if (!admitted(view) || !can(view, 'timecard.view')) return [];
  const urls = ['/api/dsp/paycom/settings'];
  if (tab === 'timecards' && hasFeature(view, 'timecard.daily')) urls.push(dailyTimecardsUrl(date));
  else if (tab === 'meal-breaks' && hasFeature(view, 'timecard.meal_breaks'))
    urls.push(mealComparisonUrl(date));
  else if (tab === 'employees' && hasFeature(view, 'timecard.employees')) {
    const hash = dspHash(view.dsp.id, 'paycom');
    const query = readUpdateState('employee-query', '', hash);
    const status = readUpdateState('employee-status', 'all', hash);
    const page = readUpdateState('employee-page', 0, hash);
    const desc = readUpdateState('employee-sort-desc', false, hash);
    const directoryUrl =
      `/api/dsp/employees?q=${encodeURIComponent(query)}&status=${status}` +
      `&limit=${performancePolicy.employeePageSize}&offset=${page * performancePolicy.employeePageSize}` +
      `&direction=${desc ? 'desc' : 'asc'}`;
    urls.push(directoryUrl);
    const selection = readUpdateState<
      { code: string; period?: { from: string; to: string } | null } | undefined
    >('employee-selection', undefined, hash);
    const directory = dataCache.peek(directoryUrl).data as EmployeesResponse | undefined;
    const employee =
      directory?.employees.find((person) => person.code === selection?.code) ??
      directory?.employees[0];
    const code = directory ? employee?.code : selection?.code;
    const period = code === selection?.code ? selection?.period : null;
    if (code) urls.push(employeeTimecardUrl(code, period));
  } else return [];
  return urls;
}

export function prefetchTimecardTab(tab: string, date: string, view: DspView, immediate = false) {
  warm(timecardUrls(tab, date, view), 'route:paycom', immediate);
}

function selectedTimecardTab(view: DspView) {
  const preferred = readUpdateState<string | undefined>(
    'paycom-tab',
    undefined,
    dspHash(view.dsp.id, 'paycom'),
  );
  const choices = [
    ['timecards', 'timecard.daily'],
    ['meal-breaks', 'timecard.meal_breaks'],
    ['employees', 'timecard.employees'],
  ] as const;
  const shown = choices.filter(([, feature]) => hasFeature(view, feature));
  return shown.find(([tab]) => tab === preferred)?.[0] ?? shown[0]?.[0];
}

/** A warm return may commit immediately only when every selected read belongs to this admission. */
export function isTimecardDataReady(view: DspView) {
  const tab = selectedTimecardTab(view);
  if (!tab) return false;
  const urls = timecardUrls(tab, selectedPaycomDate(view.dsp.id, view.dsp.timezone), view);
  return urls.length > 0 && urls.every((url) => dataCache.peek(url).data !== undefined);
}

/** The selected tab's reads, on the selected day. */
export function prefetchTimecard(view: DspView, immediate: boolean) {
  const tab = selectedTimecardTab(view);
  if (tab)
    prefetchTimecardTab(tab, selectedPaycomDate(view.dsp.id, view.dsp.timezone), view, immediate);
}
