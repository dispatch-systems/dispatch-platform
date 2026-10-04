import { createElement, lazy } from 'react';
import { Bot, Building2, FlaskConical, ScrollText, Settings } from 'lucide-react';
import {
  auditWording,
  loadPlatformSlots,
  type Access,
  type FrontendFeature,
} from '../../shell/frontend/runtime/slots.js';
import { preloadTab } from './settings/tabs.js';

declare module '../../shell/frontend/runtime/slots.js' {
  interface PlatformPages {
    dsps: true;
    jobs: true;
    agents: true;
    authorize: true;
    audit: true;
    account: true;
  }
}

// The DSPs page opens with what every owner puts in its slots, such as each switch's icon.
const loadDsps = () =>
  Promise.all([import('./dsps/index.js'), loadPlatformSlots()]).then(([module]) => module);
const loadPicker = () => import('./dsps/picker.js');
// Diagnostics words each collection's runs as an owner does.
const loadDiagnostics = () =>
  Promise.all([import('./diagnostics/index.js'), loadPlatformSlots()]).then(([module]) => module);
const loadAgents = () => import('./agents/index.js');
// The audit log is handed every owner's wording.
const loadAudit = () =>
  loadPlatformSlots()
    .then(() => import('./audit/index.js'))
    .then((module) => {
      module.installWording(auditWording());
      return module;
    });
// The page and the tab it opens on load together, so the page opens whole.
const loadSettings = () =>
  Promise.all([import('./settings/index.js'), preloadTab()]).then(([module]) => module);
const DspsPage = lazy(() => loadDsps().then((module) => ({ default: module.DspsPage })));
const DspPicker = lazy(() => loadPicker().then((module) => ({ default: module.DspPicker })));
const DiagnosticsPage = lazy(() =>
  loadDiagnostics().then((module) => ({ default: module.DiagnosticsPage })),
);
const AgentsPage = lazy(() => loadAgents().then((module) => ({ default: module.AgentsPage })));
const AuthorizePage = lazy(() =>
  loadAgents().then((module) => ({ default: module.AuthorizePage })),
);
const AuditPage = lazy(() => loadAudit().then((module) => ({ default: module.AuditPage })));
const SettingsPage = lazy(() =>
  loadSettings().then((module) => ({ default: module.SettingsPage })),
);

const platformOwner = ({ session }: Access) => session.user.platformOwner;

export const feature: FrontendFeature = {
  name: 'platform_owner',
  routes: [
    {
      // A member's one platform page: the DSPs they belong to.
      id: 'dsps',
      scope: 'platform',
      label: 'DSPs',
      icon: Building2,
      nav: true,
      preload: (access) => (access?.session.user.platformOwner ? loadDsps() : loadPicker()),
      render: ({ session }) =>
        session.user.platformOwner
          ? createElement(DspsPage)
          : createElement(DspPicker, { session }),
      prefetch: ({ session, warm }) => {
        if (session?.user.platformOwner) warm(['/api/platform/dsps']);
      },
    },
    {
      id: 'jobs',
      scope: 'platform',
      label: 'Diagnostics',
      icon: FlaskConical,
      nav: true,
      permission: platformOwner,
      preload: loadDiagnostics,
      render: () => createElement(DiagnosticsPage),
    },
    {
      id: 'agents',
      scope: 'platform',
      label: 'Agents',
      icon: Bot,
      nav: true,
      permission: platformOwner,
      preload: loadAgents,
      render: () => createElement(AgentsPage),
    },
    {
      id: 'authorize',
      scope: 'platform',
      label: 'Connect an app',
      parent: 'agents',
      nav: false,
      permission: platformOwner,
      preload: loadAgents,
      render: () => createElement(AuthorizePage),
    },
    {
      id: 'audit',
      scope: 'platform',
      label: 'Audit log',
      icon: ScrollText,
      nav: true,
      permission: platformOwner,
      preload: loadAudit,
      render: () => createElement(AuditPage),
    },
    {
      id: 'account',
      scope: 'platform',
      label: 'Settings',
      icon: Settings,
      nav: platformOwner,
      preload: loadSettings,
      render: ({ session }) => createElement(SettingsPage, { session }),
    },
  ],
};
