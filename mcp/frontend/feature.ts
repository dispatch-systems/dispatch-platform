import { createElement, lazy } from 'react';
import { Bot } from 'lucide-react';
import type { Access, FrontendFeature } from '../../core/shell/frontend/runtime/slots.js';

// The MCP's frontend manifest, loaded up front: it stays small and loads its pages lazily.

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
  errors: {
    agent_key_name_taken: 'Another key already uses this name.',
    agent_key_limit: 'You can have up to 50 keys in use. Revoke one first.',
    agent_key_revoked: 'This key was revoked. Make a new one instead.',
    agent_key_not_found: 'This key no longer exists. Refresh the page.',
    invalid_expiry: 'Choose an expiry between tomorrow and five years from now.',
  },
};
