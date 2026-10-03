import type { FrontendFeature } from '../../core/shell/frontend/runtime/slots.js';
import { feature as home } from '../../features/home/frontend/feature.js';
import { feature as timecard } from '../../features/timecard/frontend/feature.js';
import { feature as uniforms } from '../../features/uniforms/frontend/feature.js';
import { feature as dvic } from '../../features/dvic/frontend/feature.js';
import { feature as team } from '../../features/team/frontend/feature.js';
import { feature as settings } from '../../features/settings/frontend/feature.js';
import { feature as platformOwner } from '../../core/platform_owner/frontend/feature.js';

// Every owner's frontend manifest: the features, then core's parts with screens. The order is
// the sidebar's.
export const features: readonly FrontendFeature[] = [
  home,
  timecard,
  uniforms,
  dvic,
  team,
  settings,
  platformOwner,
];
