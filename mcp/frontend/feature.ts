import { createElement, lazy } from 'react';
import { Bot } from 'lucide-react';
import type { Access, FrontendFeature } from '../../core/shell/frontend/runtime/slots.js';

// The MCP's frontend manifest, loaded up front: it stays small and loads its pages lazily.
// Its error codes are worded in its API client, which loads with its pages: only they make
// the calls that raise them.

declare module '../../core/shell/frontend/runtime/slots.js' {
  interface PlatformPages {
    agents: true;
    authorize: true;
  }
}

const loadAgents = () => import('./index.js');
const AgentsPage = lazy(() => loadAgents().then((module) => ({ default: module.AgentsPage })));
const AuthorizePage = lazy(() =>
  loadAgents().then((module) => ({ default: module.AuthorizePage })),
);

const platformOwner = ({ session }: Access) => session.user.platformOwner;

export const feature: FrontendFeature = {
  name: 'mcp',
  routes: [
    {
      id: 'agents',
      scope: 'platform',
      label: 'Agents',
      icon: Bot,
      nav: true,
      after: 'jobs',
      permission: platformOwner,
      preload: loadAgents,
      render: () => createElement(AgentsPage),
    },
    {
      // An app asking to connect sends the owner here, signed in or not.
      id: 'authorize',
      scope: 'platform',
      label: 'Connect an app',
      parent: 'agents',
      nav: false,
      linked: true,
      permission: platformOwner,
      preload: loadAgents,
      render: () => createElement(AuthorizePage),
    },
  ],
  platformSlots: () => import('./platform-slots.js'),
};
