// Its page of a workspace's settings, and its own sections of them, loaded when the page opens.
import './slack.css';
import './workspace/workspace.css';
import './integrations/integrations.css';
import './linked/linked.css';
export { CanvasSummary } from '../integrations/sync_canvases/frontend/CanvasSummary.js';
export { SyncCanvasesSection } from '../integrations/sync_canvases/frontend/SyncCanvasesSection.js';
export { ChannelsSection } from './workspace/ChannelsSection.js';
export { IntegrationsSection } from './integrations/IntegrationsSection.js';
export { ImportFilesSection } from '../integrations/import_files/frontend/ImportFilesSection.js';
export { ImportSummary } from '../integrations/import_files/frontend/ImportSummary.js';
export { WorkspacePage } from './workspace/WorkspacePage.js';
