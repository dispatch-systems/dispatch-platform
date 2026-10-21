import type { CommunicationsWorkspace } from '../../api/index.js';
import { workspaceLogoUrl } from '../../api/client.js';
import { WorkspaceLogo } from '../../../slack/frontend/index.js';

/** One of the DSP's workspaces' mark, sized by `className`: its logo, else its initials. */
export function WorkspaceMark({
  workspace,
  className = 'workspace-mark',
}: {
  workspace: CommunicationsWorkspace;
  className?: string;
}) {
  return (
    <WorkspaceLogo
      name={workspace.name}
      src={workspace.logo ? workspaceLogoUrl(workspace.teamId, workspace.logo) : undefined}
      className={className}
    />
  );
}
