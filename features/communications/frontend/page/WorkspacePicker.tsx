import { Check, ChevronDown } from 'lucide-react';
import type { CommunicationsWorkspace } from '../../api/index.js';
import { workspaceLogoUrl } from '../../api/client.js';
import { SlackLogo, WorkspaceLogo } from '../../../slack/frontend/index.js';
import { Popover } from '../../../../core/shell/frontend/ui/index.js';
import { initials, tone } from '../format.js';

// Which workspace the lists show: every one, or one of them, each with how many of its
// conversations wait on an answer. Only with two or more where Communications works.

const ALL = 'All workspaces';

export function WorkspacePicker({
  workspaces,
  team,
  waiting,
  choose,
}: {
  workspaces: readonly CommunicationsWorkspace[];
  /** The workspace chosen; none for every one. */
  team?: string;
  /** How many conversations wait, in all, and in each workspace by its team ID. */
  waiting: { all: number; in: ReadonlyMap<string, number> };
  choose: (team?: string) => void;
}) {
  const chosen = workspaces.find((workspace) => workspace.teamId === team);
  return (
    <Popover
      className="workspace-picker"
      triggerClassName="workspace-button"
      label={`Workspace: ${chosen?.name ?? ALL}`}
      trigger={
        <>
          <Mark workspace={chosen} />
          <span className="workspace-name">{chosen?.name ?? ALL}</span>
          <ChevronDown size={16} aria-hidden="true" />
        </>
      }
    >
      <Choice count={waiting.all} current={!chosen} pick={() => choose(undefined)} />
      {workspaces.map((workspace) => (
        <Choice
          key={workspace.teamId}
          workspace={workspace}
          count={waiting.in.get(workspace.teamId) ?? 0}
          current={workspace === chosen}
          pick={() => choose(workspace.teamId)}
        />
      ))}
    </Popover>
  );
}

/** A workspace to choose, or every one: its mark, its name, how many wait, and whether it's chosen. */
function Choice({
  workspace,
  count,
  current,
  pick,
}: {
  workspace?: CommunicationsWorkspace;
  count: number;
  current: boolean;
  pick: () => void;
}) {
  return (
    <button aria-current={current || undefined} onClick={pick}>
      <Mark workspace={workspace} />
      <span className="workspace-name">{workspace?.name ?? ALL}</span>
      {count > 0 && <span className="count">{count}</span>}
      {current && <Check className="workspace-check" size={15} aria-hidden="true" />}
    </button>
  );
}

/**
 * A workspace's mark: its logo, or the first letters of its name in its tone until it comes or
 * without one; Slack's for every one.
 */
function Mark({ workspace }: { workspace?: CommunicationsWorkspace }) {
  return workspace ? (
    <WorkspaceLogo
      src={workspace.logo ? workspaceLogoUrl(workspace.teamId, workspace.logo) : undefined}
      className={`workspace-mark tone-${tone(workspace.name)}`}
    >
      {initials(workspace.name)}
    </WorkspaceLogo>
  ) : (
    <span className="workspace-mark all" aria-hidden="true">
      <SlackLogo size={14} />
    </span>
  );
}
