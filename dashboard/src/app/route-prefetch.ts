import type { DspView, EmployeesResponse, SessionView } from '../../../shared/contracts/index.js';
import { api, view as admittedToken } from './api.js';
import { readUpdateState } from './browser-update.js';
import { dataCache } from './data-cache.js';
import { dailyTimecardsUrl, employeeTimecardUrl, mealComparisonUrl } from './endpoints.js';
import { hasFeature } from './features.js';
import { dspHash, hashQuery, parseHash } from './navigation.js';
import { can } from './permissions.js';
import { canPrefetch, prefetchData } from './prefetch.js';
import { selectedPaycomDate } from '../lib/paycom-date.js';
import { performancePolicy } from '../lib/performance-policy.js';

function warm(urls: string[], owner: string, immediate: boolean) {
  if (immediate) {
    if (document.hidden || !navigator.onLine) return;
    for (const url of urls)
      void dataCache.read(url, (signal) => api(url, undefined, signal, 'high')).catch(() => {});
  } else if (canPrefetch()) prefetchData(urls, { owner, priority: true });
}
const admitted = (view?: DspView): view is DspView => Boolean(view && admittedToken === view.token);

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

export function prefetchSettingsTab(
  tab: string,
  view?: DspView,
  _session?: SessionView,
  immediate = false,
) {
  if (!admitted(view)) return;
  const urls: string[] = [];
  if (can(view, 'driver_match.manage'))
    urls.push(tab === 'driver-match' ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts');
  if (tab === 'data' && can(view, 'routes.manage')) urls.push('/api/dsp/routes/retention');
  if (tab === 'connections' && can(view, 'connections.manage')) {
    if (view.features.includes('paycom')) urls.push('/api/dsp/connections');
    if (view.features.includes('cortex')) urls.push('/api/dsp/connections/cortex');
  }
  warm(urls, 'route:settings', immediate);
}

export function prefetchRouteData(
  page: string,
  view?: DspView,
  session?: SessionView,
  immediate = false,
) {
  if (!view) {
    if (!admittedToken && session?.user.platformOwner && page === 'dsps')
      warm(['/api/platform/dsps'], 'route:dsps', immediate);
    return;
  }
  if (!admitted(view)) return;
  if (page === 'paycom') {
    const tab = selectedTimecardTab(view);
    if (tab)
      prefetchTimecardTab(tab, selectedPaycomDate(view.dsp.id, view.dsp.timezone), view, immediate);
  } else if (page === 'dvic' && can(view, 'dvic.view'))
    warm(['/api/dsp/dvic/status'], 'route:dvic', immediate);
  else if (page === 'paycom-settings' && can(view, 'timecard.manage'))
    warm(['/api/dsp/schedules'], 'route:paycom-settings', immediate);
  else if (page === 'team') {
    if (can(view, 'members.invite') || can(view, 'members.manage') || can(view, 'roles.manage'))
      warm(['/api/dsp/members', '/api/dsp/roles'], 'route:team', immediate);
  } else if (page === 'settings') {
    const address = parseHash(location.hash);
    const tab =
      address.dspId === view.dsp.id && address.page === page ? hashQuery().get('tab') : null;
    prefetchSettingsTab(tab ?? 'general', view, session, immediate);
  }
}
