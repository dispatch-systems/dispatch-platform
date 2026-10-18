import { useCallback, useEffect } from 'react';
import { useData } from '../../../../../core/shell/frontend/runtime/api.js';
import { call, channelLinking, workspaceUrl } from '../../../api/client.js';
import type { SlackImports } from '../../../api/generated/SlackImports.js';

// Import Files's endpoints, as its section and its card in a workspace's settings call them.

/** Import Files in the workspace `team`: where files go, the channels linked, and the latest files. */
const importReaders = new Set<() => void>();
export function useSlackImports(team: string) {
  const url = `${workspaceUrl(team)}/imports`;
  const read = useCallback(
    (signal: AbortSignal) => call<SlackImports>(url, undefined, signal),
    [url],
  );
  // Files arrive from Slack on their own, so an open section reads them again every 10 seconds.
  const imports = useData<SlackImports>(url, 10000, undefined, undefined, false, read);
  const { refresh } = imports;
  useEffect(() => {
    importReaders.add(refresh);
    return () => void importReaders.delete(refresh);
  }, [refresh]);
  return imports;
}
/** Has every reading of Import Files read it again, after a change. */
const importsChanged = <T>(answer: T) => {
  for (const read of importReaders) read();
  return answer;
};
/** Import Files' channels: their files from now on saved to their folders. */
export const importLinking = channelLinking<SlackImports>('imports', importsChanged);
