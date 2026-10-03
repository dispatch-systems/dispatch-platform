import { createElement, lazy } from 'react';
import { Users } from 'lucide-react';
import type { DspView } from '../../../shared/contracts/index.js';
import { can } from '../../../core/shell/frontend/runtime/permissions.js';
import type { FrontendFeature } from '../../../core/shell/frontend/runtime/slots.js';

declare module '../../../core/shell/frontend/runtime/slots.js' {
  interface DspPages {
    team: true;
  }
}

const load = () => import('./index.js');
const TeamPage = lazy(() => load().then((module) => ({ default: module.TeamPage })));
const manages = (view?: DspView) =>
  can(view, 'members.invite') || can(view, 'members.manage') || can(view, 'roles.manage');

export const feature: FrontendFeature = {
  name: 'team',
  routes: [
    {
      id: 'team',
      scope: 'dsp',
      label: 'Team & Roles',
      icon: Users,
      nav: true,
      permission: ({ view }) => manages(view),
      preload: load,
      render: ({ view, reopen }) => createElement(TeamPage, { view, reopen }),
      prefetch: ({ view, warm }) => {
        if (manages(view)) warm(['/api/dsp/members', '/api/dsp/roles']);
      },
    },
  ],
};
