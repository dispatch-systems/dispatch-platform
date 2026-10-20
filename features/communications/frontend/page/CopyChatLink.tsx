import { Copy } from 'lucide-react';
import type { CommunicationsWorkspace } from '../../api/index.js';
import { toClipboard } from '../../../../core/shell/frontend/runtime/clipboard.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';

// Copy chat link: the address that opens the DSP's chat with Dispatch in Slack, for employees
// to write to it. With one workspace it's copied at once; with several, picked from a menu.

export function CopyChatLink({ workspaces }: { workspaces: readonly CommunicationsWorkspace[] }) {
  const copy = useAction((workspace: CommunicationsWorkspace) => toClipboard(workspace.chatLink), {
    success: 'Chat link copied',
  });
  const label = (
    <>
      <Copy size={16} />
      <span>Copy chat link</span>
    </>
  );
  const [only] = workspaces;
  if (workspaces.length === 1 && only)
    return (
      <button aria-label="Copy chat link" onClick={() => void copy.run(only)}>
        {label}
      </button>
    );
  return (
    <Popover
      className="chat-link-menu"
      triggerClassName="chat-link-button"
      label="Copy chat link"
      trigger={label}
    >
      {workspaces.map((workspace) => (
        <button key={workspace.teamId} onClick={() => void copy.run(workspace)}>
          {workspace.name}
        </button>
      ))}
    </Popover>
  );
}
