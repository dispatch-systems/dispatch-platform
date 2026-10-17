import { Suspense, useState } from 'react';
import type { DspView, SessionView } from '../../../../core/accounts/api/index.js';
import {
  hashQuery,
  replaceHashQuery,
  settingsHash,
} from '../../../../core/shell/frontend/runtime/navigation.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import {
  connectionSections,
  type ConnectionSection,
} from '../../../../core/shell/frontend/runtime/slots.js';
import { DataState, ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useSlackAccount, type SlackWorkspace } from '../../api/client.js';
import { INTEGRATIONS } from '../sections.js';
import { WorkspaceHero } from './WorkspaceHero.js';

// One of the DSP's workspaces' settings, on a page of their own, for those who manage Slack: the
// workspace at the top, and its sections down the side, Slack's own and those other features add
// for each workspace, one at a time. An integration's sections are listed under Integrations,
// while it's on in the workspace. A feature switched off for the DSP takes its sections with it.
// The address names the workspace and the section open.

export function WorkspacePage({ session, view }: { session: SessionView; view: DspView }) {
  const team = hashQuery().get('workspace') ?? '';
  const account = useSlackAccount();
  const [asked, setAsked] = useState(hashQuery().get('section') ?? '');
  const open = (section: string) => {
    replaceHashQuery({ workspace: team, section });
    setAsked(section);
  };
  const named = account.data?.workspaces.find((each) => each.teamId === team);
  return (
    <div className="slack-page">
      <nav className="slack-trail" aria-label="Breadcrumb">
        <a href={settingsHash(view.dsp.id)}>Settings</a>
        <span aria-hidden="true">›</span>
        <a href={settingsHash(view.dsp.id, 'connections')}>Connections</a>
        {named && (
          <>
            <span aria-hidden="true">›</span>
            <span aria-current="page">{named.label ?? named.name}</span>
          </>
        )}
      </nav>
      <DataState data={account.data} error={account.error} retry={account.refresh}>
        {(data) => {
          const workspace = data.workspaces.find((each) => each.teamId === team);
          if (!workspace)
            return <ErrorBox message="That workspace isn't connected. Refresh the page." />;
          const sections = listed(connectionSections('slack', view), workspace);
          const section = sections.find((each) => each.id === asked) ?? sections[0];
          return (
            <>
              <WorkspaceHero
                workspace={workspace}
                timeZone={view.dsp.timezone}
                renames={can(view, 'connections.manage')}
              />
              <div className="slack-page-body">
                <nav className="slack-sections" aria-label="Sections">
                  {sections.map(({ id, label, icon: Icon, integration }) => (
                    <button
                      key={id}
                      className={integration ? 'slack-subsection' : undefined}
                      aria-current={id === section?.id ? 'page' : undefined}
                      onClick={() => open(id)}
                    >
                      {Icon && <Icon size={16} />}
                      {label}
                    </button>
                  ))}
                </nav>
                {section && (
                  <section className="slack-section" aria-label={section.label}>
                    <h2>{section.label}</h2>
                    <Suspense fallback={<Loading />}>
                      {section.render({ session, view, account: team, open })}
                    </Suspense>
                  </section>
                )}
              </div>
            </>
          );
        }}
      </DataState>
    </div>
  );
}

/**
 * The sections listed for `workspace`, in their order: an integration's only while it's on
 * there, right under Integrations.
 */
function listed(sections: ConnectionSection[], workspace: SlackWorkspace) {
  const integrations = sections.filter(
    (section) => section.integration && workspace.integrations.includes(section.integration),
  );
  return sections
    .filter((section) => !section.integration)
    .flatMap((section) => (section.id === INTEGRATIONS ? [section, ...integrations] : [section]));
}
