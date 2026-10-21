import { Suspense, useEffect, useState } from 'react';
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
import { DataState, Empty, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useSlackAccount, type SlackWorkspace } from '../../api/client.js';
import { INTEGRATIONS } from '../sections.js';
import { WorkspaceHero } from './WorkspaceHero.js';
import { WorkspacePick } from './WorkspacePick.js';

// The DSP's workspaces' settings, Slack's custom section of its page of Settings, for those who
// manage Slack: one workspace at a time, chosen from them when there are several, with its
// sections down the side, Slack's own and those other features add for each workspace. An
// integration's sections are listed under Integrations, while it's on in the workspace. A
// feature switched off for the DSP takes its sections with it. The address names the workspace
// and the section open, beside Settings' own page.

export function Workspaces({ session, view }: { session: SessionView; view: DspView }) {
  const account = useSlackAccount();
  const [chosen, choose] = useChosen();
  const connections = settingsHash(view.dsp.id, 'connections', view);
  return (
    <div className="slack-page">
      <DataState data={account.data} error={account.error} retry={account.refresh}>
        {(data) => {
          const workspace =
            data.workspaces.find((each) => each.teamId === chosen.workspace) ?? data.workspaces[0];
          if (!workspace)
            return (
              <Empty
                title="No Slack workspaces"
                action={
                  connections && (
                    <a className="slack-link-button" href={connections}>
                      Connections
                    </a>
                  )
                }
              />
            );
          const sections = listed(connectionSections('slack', view), workspace);
          const section = sections.find((each) => each.id === chosen.section) ?? sections[0];
          const open = (id: string) => choose({ workspace: workspace.teamId, section: id });
          return (
            <>
              {data.workspaces.length > 1 && (
                <WorkspacePick
                  workspaces={data.workspaces}
                  open={workspace}
                  choose={(team) => choose({ workspace: team, section: '' })}
                />
              )}
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
                      {section.render({ session, view, account: workspace.teamId, open })}
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

type Chosen = { workspace: string; section: string };
/** The workspace and the section the address names. */
const named = (): Chosen => ({
  workspace: hashQuery().get('workspace') ?? '',
  section: hashQuery().get('section') ?? '',
});
/**
 * The workspace and the section the address names, followed as it moves, and `choose` to open
 * others, which records them in the address beside what else it names, as Settings' page.
 */
function useChosen() {
  const [chosen, setChosen] = useState(named);
  useEffect(() => {
    const follow = () => setChosen(named());
    window.addEventListener('hashchange', follow);
    return () => window.removeEventListener('hashchange', follow);
  }, []);
  const choose = (next: Chosen) => {
    const query = hashQuery();
    query.set('workspace', next.workspace);
    if (next.section) query.set('section', next.section);
    else query.delete('section');
    replaceHashQuery(Object.fromEntries(query));
    setChosen(next);
  };
  return [chosen, choose] as const;
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
