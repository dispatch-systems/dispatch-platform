import { installFeatures } from '../../../core/shell/frontend/runtime/slots.js';
import { features } from '../../frontend/features.js';

// Every owner's frontend manifest, installed as the app installs them before its first render.
// A test imports this before any module that reads the slots as it loads.
installFeatures(features);
