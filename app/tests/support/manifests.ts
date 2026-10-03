import { installFeatures, loadPlatformSlots } from '../../../core/shell/frontend/runtime/slots.js';
import { features } from '../../frontend/features.js';

// Every owner's frontend manifest, installed as the app installs them before its first render,
// and what each puts in the platform owner's slots, loaded as those pages load it. A test imports
// this first, and a module that reads the slots as it loads in its body, once this has run.
installFeatures(features);
await loadPlatformSlots();
