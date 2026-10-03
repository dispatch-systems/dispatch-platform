import type { ComponentType } from 'react';
import type { DspView } from '../../../shared/contracts/index.js';
import { hashQuery, parseHash } from '../../../core/shell/frontend/runtime/navigation.js';
import { admitted, warm } from '../../../core/shell/frontend/runtime/route-prefetch.js';
import { settingsTabs } from '../../../core/shell/frontend/runtime/slots.js';

// A DSP's Settings is made of the tabs its owners contribute: the account's own, and each
// feature's settings panel.

/** The tabs this person sees, in order. */
export const visibleTabs = (view?: DspView) =>
  settingsTabs()
    .filter((tab) => !tab.visible || tab.visible(view))
    .sort((a, b) => a.order - b.order);

const badges = new Map<string, ComponentType<{ active: boolean }>>();
/** A tab's badge, once loaded. */
export const loadedBadge = (id: string) => badges.get(id);

/** The code of the tab the address names, so the page opens on it, and the tabs' badges. */
export function preloadSettingsTab(view?: DspView) {
  const tabs = visibleTabs(view);
  const id = hashQuery().get('tab') || tabs[0]?.id;
  return Promise.all([
    ...tabs.map((tab) =>
      tab.badge
        ?.load()
        .then((Label) => void badges.set(tab.id, Label))
        .catch(() => undefined),
    ),
    tabs.find((tab) => tab.id === id)?.load(),
  ]);
}

/** The reads a Settings tab opens on, warmed when the tab may open next. */
export function prefetchSettingsTab(tab: string, view?: DspView, immediate = false) {
  if (!admitted(view)) return;
  warm(
    settingsTabs().flatMap((contribution) => contribution.prefetch?.(tab, view) ?? []),
    'route:settings',
    immediate,
  );
}

/** The tab the address names, while it names this DSP's Settings. */
export function prefetchSettings(view: DspView, immediate: boolean) {
  const address = parseHash(location.hash);
  const tab =
    address.dspId === view.dsp.id && address.page === 'settings' ? hashQuery().get('tab') : null;
  prefetchSettingsTab(tab ?? visibleTabs(view)[0]?.id ?? '', view, immediate);
}
