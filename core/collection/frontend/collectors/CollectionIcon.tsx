import { createElement } from 'react';
import { CalendarSync, type LucideProps } from 'lucide-react';
import { featureIcon } from '../../../shell/frontend/runtime/slots.js';

/** A collection's icon: that of the feature that keeps it, as its pages show it. */
export const CollectionIcon = ({ feature, ...props }: { feature: string } & LucideProps) =>
  createElement(featureIcon(feature) ?? CalendarSync, { 'aria-hidden': true, ...props });
