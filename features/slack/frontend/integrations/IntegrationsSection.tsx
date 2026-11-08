import { useState } from 'react';
import {
  connectionIntegrations,
  connectionSections,
  type ConnectionSectionContext,
  type ListedIntegration,
} from '../../../../core/shell/frontend/runtime/slots.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { DataState, ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { switchSlackIntegration, useSlackAccount } from '../../api/client.js';
import { IntegrationCard } from './IntegrationCard.js';
import { IntegrationSwitch } from './IntegrationSwitch.js';

// Slack's section of a workspace's settings where what other features do through Slack is
// switched on and off there, each on its own card. Each says what switching it means before
// it's switched, and shows switched as soon as that's asked for. A feature adds one by declaring
// it: nothing here names any.

export function IntegrationsSection({
  session,
  view,
  account: team,
  open,
}: ConnectionSectionContext) {
  const account = useSlackAccount();
  // The integration being switched, and to what, while its switch asks.
  const [asked, setAsked] = useState<{ integration: ListedIntegration; on: boolean }>();
  const switching = useAction(
    (integration: ListedIntegration, on: boolean) =>
      switchSlackIntegration(team, integration.id, on),
    { inline: true },
  );
  if (!view) return null;
  const integrations = connectionIntegrations('slack', view);
  const sections = connectionSections('slack', view);
  return (
    <DataState data={account.data} error={account.error} retry={account.refresh}>
      {(data) => {
        const workspace = data.workspaces.find((each) => each.teamId === team);
        if (!workspace) return null;
        return (
          <div className="slack-integrations">
            <ErrorBox message={switching.error} />
            {integrations.map((integration) => (
              <IntegrationCard
                key={integration.id}
                integration={integration}
                context={{ session, view, account: team }}
                on={workspace.integrations.includes(integration.id)}
                locked={workspace.status === 'broken'}
                settings={sections.find((section) => section.integration === integration.id)}
                switchTo={(on) => setAsked({ integration, on })}
                open={open}
              />
            ))}
            {asked && (
              <IntegrationSwitch
                integration={asked.integration}
                on={asked.on}
                confirm={() => {
                  setAsked(undefined);
                  void switching.run(asked.integration, asked.on);
                }}
                close={() => setAsked(undefined)}
              />
            )}
          </div>
        );
      }}
    </DataState>
  );
}
