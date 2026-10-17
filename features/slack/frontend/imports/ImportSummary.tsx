import type { DspView } from '../../../../core/accounts/api/index.js';
import { can } from '../../../../core/shell/frontend/runtime/permissions.js';
import { offered } from '../../../../core/shell/frontend/runtime/services.js';
import { useSlackImports } from '../../api/client.js';

// What Import Files is doing in a workspace, on its card while it's on: how many channels are
// linked and how many files they've saved. Only for those who see where files are kept.

export function ImportSummary({ view, team }: { view: DspView; team: string }) {
  const kept = offered('files', view);
  if (!kept || !can(view, kept.permission)) return null;
  return <Counts team={team} />;
}

function Counts({ team }: { team: string }) {
  const { data } = useSlackImports(team);
  if (!data) return null;
  const channels = data.links.length;
  const saved = data.links.reduce((total, link) => total + link.saved, 0);
  return (
    <>
      {channels} {channels === 1 ? 'channel' : 'channels'} · {saved}{' '}
      {saved === 1 ? 'file' : 'files'} saved
    </>
  );
}
