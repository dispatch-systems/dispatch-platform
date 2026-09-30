/** The features a DSP may have, mirroring the catalog in `backend/src/features.rs`. */
export const pages = ['timecard', 'uniforms', 'routes', 'dvic'] as const;
export const pageTabs = [
  'timecard.daily',
  'timecard.meal_breaks',
  'timecard.employees',
  'dvic.day',
  'dvic.week',
] as const;
export const connections = ['paycom', 'cortex'] as const;
export const features = [...pages, ...pageTabs, ...connections] as const;
export type Feature = (typeof features)[number];
export type PageFeature = (typeof pages)[number];
export type TabFeature = (typeof pageTabs)[number];
export type ConnectionFeature = (typeof connections)[number];
export type { DspFeatures } from './generated/DspFeatures';
export type { FeatureChange } from './generated/FeatureChange';
export type { FeatureState } from './generated/FeatureState';
export type { DspFeatureReport } from './generated/DspFeatureReport';
