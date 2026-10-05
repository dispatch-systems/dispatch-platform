import {
  Award,
  CalendarDays,
  ClipboardCheck,
  Route,
  Utensils,
  type LucideIcon,
} from 'lucide-react';
import type { DriverData } from '../api/index.js';

export const dataIcons: Record<DriverData, LucideIcon> = {
  timecards: CalendarDays,
  routes: Route,
  meal_breaks: Utensils,
  dvic: ClipboardCheck,
  weekly_scorecard: Award,
};
