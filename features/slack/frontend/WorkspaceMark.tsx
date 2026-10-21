import { workspaceLogoUrl, type SlackWorkspace } from '../api/client.js';
import { WorkspaceLogo } from './WorkspaceLogo.js';

/** One of the DSP's workspaces' mark on Slack's own screens, sized by `className`. */
export function WorkspaceMark({
  workspace,
  className,
}: {
  workspace: SlackWorkspace;
  className: string;
}) {
  return (
    <WorkspaceLogo
      name={workspace.label ?? workspace.name}
      src={workspace.logo ? workspaceLogoUrl(workspace.teamId, workspace.logo) : undefined}
      className={className}
    />
  );
}
