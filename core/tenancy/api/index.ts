// The DSPs' features: generated from the backend catalog; the closed ID sets stay in catalog order.
import { features, pages, subfeatures, connections } from './generated/access-catalog.js';
export { features, pages, subfeatures, connections };
export type Feature = (typeof features)[number];
export type PageFeature = (typeof pages)[number];
export type SubFeature = (typeof subfeatures)[number];
export type ConnectionFeature = (typeof connections)[number];
