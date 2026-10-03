import type { DspView } from '../../../shared/contracts/index.js';
import { hashQuery, parseHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import { admitted, warm } from '../../../core/shell/frontend/runtime/route-prefetch.js';

/** The reads a Settings tab opens on, warmed when the tab may open next. */
export function prefetchSettingsTab(tab: string, view?: DspView, immediate = false) {
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

/** The tab the address names, while it names this DSP's Settings. */
export function prefetchSettings(view: DspView, immediate: boolean) {
  const address = parseHash(location.hash);
  const tab =
    address.dspId === view.dsp.id && address.page === 'settings' ? hashQuery().get('tab') : null;
  prefetchSettingsTab(tab ?? 'general', view, immediate);
}
