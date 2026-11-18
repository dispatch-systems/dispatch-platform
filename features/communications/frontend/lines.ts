import { createElement, lazy } from 'react';
import { begins } from '../../../core/shell/frontend/runtime/data-policy.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type {
  CustomSection,
  DspRoute,
  FrontendFeature,
  Watcher,
  Watching,
} from '../../../core/shell/frontend/runtime/slots.js';
import type { TextingLine } from './line.js';

// What another feature's line of the DSP's phones puts in that feature's manifest, Dispatch
// Phone's own parts drawn for its line alone (`TextingLine`): its page, the count beside it in the
// sidebar, its section of Settings' Display page, alerting its members of new messages and a
// banner while its phones are down, on every page, and how the dashboard waits on and caches its
// reads. Loaded up front with that manifest, it stays small and loads the rest lazily.

export type { TextingLine };

const loadPage = () => import('./index.js');
const LinePage = lazy(() => loadPage().then((module) => ({ default: module.CommunicationsPage })));
const loadDisplay = () => import('./display/DisplaySettings.js');
const DisplaySettings = lazy(() =>
  loadDisplay().then((module) => ({ default: module.DisplaySettings })),
);

/** The slots `line` fills: its page's, beside the id, label, icon and switch its feature gives it;
 * and its own others'. */
export function lineSlots(line: TextingLine) {
  const uses = (access: Watching) => can(access.view, line.permission);
  const page = {
    // How many of its conversations wait on an answer.
    badge: {
      load: () =>
        import('./badge.js').then(
          ({ WaitingCount }) =>
            ({ active }: { active: boolean }) =>
              createElement(WaitingCount, { active, line }),
        ),
    },
    nav: true,
    // The list and the conversation open scroll within it.
    fills: true,
    permission: ({ view }) => can(view, line.permission),
    preload: loadPage,
    render: ({ session, view }) => createElement(LinePage, { view, user: session.user.id, line }),
  } satisfies Partial<DspRoute>;
  const display: CustomSection = {
    id: line.display,
    load: loadDisplay,
    render: ({ view, refresh }) => view && createElement(DisplaySettings, { view, refresh, line }),
    prefetch: () => ['/api/dsp/settings'],
  };
  const watchers: Watcher[] = [
    {
      id: `${line.id}.alerts`,
      visible: uses,
      load: () =>
        import('./alerts/MessageAlerts.js').then(
          ({ MessageAlerts }) =>
            (access: Watching) =>
              createElement(MessageAlerts, { ...access, line }),
        ),
    },
    {
      id: `${line.id}.phones`,
      visible: uses,
      load: () =>
        import('./texting/PhonesDown.js').then(
          ({ PhonesDown }) =>
            (access: Watching) =>
              createElement(PhonesDown, { ...access, line }),
        ),
    },
  ];
  const { url } = line.texting;
  const slots = {
    settingsSections: [display],
    watchers,
    // Its page waits on changes to its conversations, and who is typing in them.
    longPolls: [`${url}/updates`],
    // Each of its writes changes its reads, but saying who is typing, which changes none.
    cache: {
      write: (write, read) =>
        begins(write, url) && !write.endsWith('/typing') ? begins(read, url) : undefined,
    },
  } satisfies Partial<FrontendFeature>;
  return { page, ...slots };
}
