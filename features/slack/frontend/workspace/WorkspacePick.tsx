import { Check, ChevronDown } from 'lucide-react';
import type { SlackWorkspace } from '../../api/client.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import { WorkspaceMark } from '../WorkspaceMark.js';

// Which of the DSP's workspaces its settings show, with two or more: each by its mark and name.

export function WorkspacePick({
  workspaces,
  open,
  choose,
}: {
  workspaces: readonly SlackWorkspace[];
  /** The workspace whose settings show. */
  open: SlackWorkspace;
  choose: (team: string) => void;
}) {
  const name = (workspace: SlackWorkspace) => workspace.label ?? workspace.name;
  return (
    <div className="slack-workspace-pick">
      <span aria-hidden="true">Workspace</span>
      <Popover
        className="slack-workspace-menu"
        triggerClassName="slack-workspace-button"
        label={`Workspace: ${name(open)}`}
        trigger={
          <>
            <WorkspaceMark workspace={open} className="slack-pick-mark" />
            <span className="slack-workspace-name">{name(open)}</span>
            <ChevronDown size={16} aria-hidden="true" />
          </>
        }
      >
        {workspaces.map((workspace) => (
          <button
            key={workspace.teamId}
            aria-current={workspace === open || undefined}
            onClick={() => choose(workspace.teamId)}
          >
            <WorkspaceMark workspace={workspace} className="slack-pick-mark" />
            <span className="slack-workspace-name">{name(workspace)}</span>
            {workspace === open && <Check size={15} aria-hidden="true" />}
          </button>
        ))}
      </Popover>
    </div>
  );
}
