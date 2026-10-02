// Generated from the backend catalog; the closed ID sets stay in catalog order.
import { features, pages, pageTabs, connections } from './generated/access-catalog.js';
export { features, pages, pageTabs, connections };
export type Feature = (typeof features)[number];
export type PageFeature = (typeof pages)[number];
export type TabFeature = (typeof pageTabs)[number];
export type ConnectionFeature = (typeof connections)[number];
export type { DspFeatures } from './generated/DspFeatures';
export type { FeatureChange } from './generated/FeatureChange';
export type { FeatureState } from './generated/FeatureState';
export type { DspFeatureReport } from './generated/DspFeatureReport';
