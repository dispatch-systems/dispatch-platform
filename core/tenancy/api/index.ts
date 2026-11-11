// The DSPs' features: generated from the backend catalog; the closed ID sets stay in catalog order.
import { features, pages, subfeatures, connections } from './generated/access-catalog.js';
import type { FeatureId, PageId, SubId } from './generated/custom-types.js';
export { features, pages, subfeatures, connections };
export type Feature = FeatureId;
export type PageFeature = PageId;
export type SubFeature = SubId;
export type ConnectionFeature = (typeof connections)[number];
