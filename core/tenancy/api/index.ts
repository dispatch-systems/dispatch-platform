// The DSPs' features: generated from the backend catalog; the closed ID sets stay in catalog order.
import {
  features,
  pages,
  pageTabs,
  connections,
} from '../../../shared/contracts/generated/access-catalog.js';
export { features, pages, pageTabs, connections };
export type Feature = (typeof features)[number];
export type PageFeature = (typeof pages)[number];
export type TabFeature = (typeof pageTabs)[number];
export type ConnectionFeature = (typeof connections)[number];
