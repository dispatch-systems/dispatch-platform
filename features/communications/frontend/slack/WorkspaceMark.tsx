import type { CommunicationsWorkspace } from '../../api/index.js';
import { workspaceLogoUrl } from '../../api/client.js';
import { SlackLogo, WorkspaceLogo } from '../../../slack/frontend/index.js';

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

/** Every workspace's mark: Slack's. */
export function AllWorkspacesMark({ className = 'workspace-mark' }: { className?: string }) {
  return (
    <span className={`${className} all`} aria-hidden="true">
      <SlackLogo size={14} />
    </span>
  );
}
