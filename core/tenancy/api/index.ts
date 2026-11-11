// The DSPs' features: generated from the backend catalog; the closed ID sets stay in catalog order.
import { features, pages, subfeatures, connections } from './generated/access-catalog.js';
import type { CustomFeatureId, CustomPageId, CustomSubId } from './generated/custom-types.js';
export { features, pages, subfeatures, connections };
export type Feature = (typeof features)[number] | CustomFeatureId;
export type PageFeature = (typeof pages)[number] | CustomPageId;
export type SubFeature = (typeof subfeatures)[number] | CustomSubId;
export type ConnectionFeature = (typeof connections)[number];
