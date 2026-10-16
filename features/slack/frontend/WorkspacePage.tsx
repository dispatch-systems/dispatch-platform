import { Suspense, useState } from 'react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import {
  hashQuery,
  replaceHashQuery,
  settingsHash,
} from '../../../core/shell/frontend/runtime/navigation.js';
import { connectionSections } from '../../../core/shell/frontend/runtime/slots.js';
import { Badge, DataState, ErrorBox, Loading } from '../../../core/shell/frontend/ui/index.js';
import { useSlackAccount, type SlackWorkspace } from '../api/client.js';
import { day } from './day.js';
import { SlackLogo } from './SlackLogo.js';

// One of the DSP's workspaces' settings, on a page of its own, for those who manage Slack: the
// workspace at the top, and its sections down the side, Slack's own and those other features
// add for each workspace, one at a time. A feature switched off for the DSP takes its sections
// with it. The address names the workspace and the section open.

export function WorkspacePage({ session, view }: { session: SessionView; view: DspView }) {
  const team = hashQuery().get('workspace') ?? '';
  const account = useSlackAccount();
  const sections = connectionSections('slack', view);
  const [asked, setAsked] = useState(hashQuery().get('section') ?? '');
  const section = sections.find((each) => each.id === asked) ?? sections[0];
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
          return (
            <>
              <Hero workspace={workspace} timeZone={view.dsp.timezone} />
              <div className="slack-page-body">
                <nav className="slack-sections" aria-label="Sections">
                  {sections.map(({ id, label, icon: Icon }) => (
                    <button
                      key={id}
                      aria-current={id === section?.id ? 'page' : undefined}
                      onClick={() => {
                        replaceHashQuery({ workspace: team, section: id });
                        setAsked(id);
                      }}
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
                      {section.render({ session, view, account: team })}
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

/** The workspace at the top of its page: its name, whether it works, and where it came from. */
function Hero({ workspace, timeZone }: { workspace: SlackWorkspace; timeZone: string }) {
  const broken = workspace.status === 'broken';
  return (
    <header className="slack-hero">
      <span className="slack-mark slack-hero-mark">
        <SlackLogo size={28} />
      </span>
      <div className="slack-hero-text">
        <h1>{workspace.label ?? workspace.name}</h1>
        <dl>
          <div>
            <dt>Workspace</dt>
            <dd>{workspace.name}</dd>
          </div>
          <div>
            <dt>Connected</dt>
            <dd>
              {day(workspace.connectedAt, timeZone)}
              {workspace.byLink
                ? ', by install link'
                : workspace.connectedBy
                  ? `, by ${workspace.connectedBy}`
                  : ''}
            </dd>
          </div>
        </dl>
      </div>
      <div className="slack-hero-status" role="status">
        {broken ? (
          <Badge value="failed">Needs Reconnecting</Badge>
        ) : workspace.needsUpdate ? (
          <Badge value="needs_verification">Needs Update</Badge>
        ) : (
          <Badge value="ready" />
        )}
      </div>
    </header>
  );
}
