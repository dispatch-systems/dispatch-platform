import { createElement, lazy } from 'react';
import { LifeBuoy, Sparkles } from 'lucide-react';
import type { FrontendFeature } from '../../shell/frontend/runtime/slots.js';

declare module '../../shell/frontend/runtime/slots.js' {
  interface PlatformPages {
    support: true;
  }
}
const loadPanel = () => import('./SupportPanel.js');
const loadOwner = () => import('./owner/SupportPage.js');
const Panel = lazy(() => loadPanel().then((module) => ({ default: module.SupportPanel })));
const Page = lazy(() => loadOwner().then((module) => ({ default: module.SupportPage })));
const Indicator = lazy(() =>
  import('./notifications/UnreadIndicator.js').then((module) => ({
    default: module.UnreadIndicator,
  })),
);
export const feature: FrontendFeature = {
  name: 'support',
  accountActions: [
    {
      id: 'ai-support',
      label: 'AI Support',
      icon: Sparkles,
      preload: loadPanel,
      render: (context) => createElement(Panel, context),
      indicator: () => createElement(Indicator),
    },
  ],
  routes: [
    {
      id: 'support',
      scope: 'platform',
      label: 'Support',
      icon: LifeBuoy,
      nav: true,
      after: 'jobs',
      permission: ({ session }) => session.user.platformOwner,
      preload: loadOwner,
      render: () => createElement(Page),
    },
  ],
  replies: () => import('../api/runtime.js'),
  cache: {
    write: (write, url) => {
      if (!write.startsWith('/api/support/') && !write.startsWith('/api/platform/support/'))
        return undefined;
      return url.startsWith('/api/support/') || url.startsWith('/api/platform/support/');
    },
  },
};
