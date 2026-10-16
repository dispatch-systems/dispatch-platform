// Generated from the kinds of data the features' people declare, and their sources.
// Run `npm run contracts:generate` after changing them.
import { Award, CalendarDays, ClipboardCheck, Contact, Route, Utensils } from 'lucide-react';
export const driverData = [
  { id: "timecards", label: "Timecards", unit: "day", icon: CalendarDays },
  { id: "driver_profiles", label: "Roster", unit: "listing", icon: Contact },
  { id: "routes", label: "Routes", unit: "day", icon: Route },
  { id: "meal_breaks", label: "Meal breaks", unit: "day", icon: Utensils },
  { id: "dvic", label: "DVIC", unit: "inspection", icon: ClipboardCheck },
  { id: "weekly_scorecard", label: "Weekly Scorecard", unit: "week", icon: Award },
  { id: "daily_performance", label: "Daily Performance", unit: "day", icon: CalendarDays },
] as const;
export const driverSources = [
  { id: "paycom", label: "Paycom" },
  { id: "amazon", label: "Amazon" },
] as const;
