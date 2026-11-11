import { channelLinking, integrationReading } from '../../../api/client.js';
import type { SlackImports } from '../../../api/generated/SlackImports.js';

// Import Files's endpoints, as its section and its card in a workspace's settings call them.

/** Import Files in the workspace `team`: where files go, the channels linked, and the latest files. */
const { useReading: useSlackImports, changed: importsChanged } =
  integrationReading<SlackImports>('imports');
export { useSlackImports };
/** Import Files' channels: their files from now on saved to their folders. */
export const importLinking = channelLinking<SlackImports>('imports', importsChanged);
