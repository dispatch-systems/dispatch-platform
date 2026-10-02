import type { DspView, SessionView } from '../../../shared/contracts/index.js';
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

export function prefetchTimecardTab(tab: string, date: string, view: DspView, immediate = false) {
  if (!admitted(view) || !can(view, 'timecard.view')) return;
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
    urls.push(
      `/api/dsp/employees?q=${encodeURIComponent(query)}&status=${status}` +
        `&limit=${performancePolicy.employeePageSize}&offset=${page * performancePolicy.employeePageSize}` +
        `&direction=${desc ? 'desc' : 'asc'}`,
    );
    const selection = readUpdateState<
      { code: string; period?: { from: string; to: string } | null } | undefined
    >('employee-selection', undefined, hash);
    if (selection?.code) urls.push(employeeTimecardUrl(selection.code, selection.period));
  } else return;
  warm(urls, 'route:paycom', immediate);
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
    const hash = dspHash(view.dsp.id, 'paycom');
    const preferred = readUpdateState<string | undefined>('paycom-tab', undefined, hash);
    const choices = [
      ['timecards', 'timecard.daily'],
      ['meal-breaks', 'timecard.meal_breaks'],
      ['employees', 'timecard.employees'],
    ] as const;
    const shown = choices.filter(([, feature]) => hasFeature(view, feature));
    const tab = shown.find(([tab]) => tab === preferred)?.[0] ?? shown[0]?.[0];
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
