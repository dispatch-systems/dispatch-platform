// Generated from the kinds of data the features' people declare, their sources and the titles a person may be given.
// Run `npm run contracts:generate` after changing them.
import { Award, CalendarDays, ClipboardCheck, Contact, MessageSquare, Route, Utensils } from 'lucide-react';
export const peopleKinds = [
  { id: "timecards", label: "Timecards", order: 10, unit: "day", icon: CalendarDays },
  { id: "driver_profiles", label: "Roster", order: 15, unit: null, icon: Contact },
  { id: "routes", label: "Routes", order: 20, unit: "day", icon: Route },
  { id: "routes_live", label: "Live routes", order: 21, unit: "day", icon: Route },
  { id: "meal_breaks", label: "Meal breaks", order: 30, unit: "day", icon: Utensils },
  { id: "weekly_scorecard", label: "Weekly Scorecard", order: 50, unit: "week", icon: Award },
  { id: "daily_performance", label: "Daily Performance", order: 60, unit: "day", icon: CalendarDays },
  { id: "slack", label: "Slack", order: 70, unit: null, icon: MessageSquare },
  { id: "dvic", label: "DVIC", order: 90, unit: "inspection", icon: ClipboardCheck },
] as const;
export const peopleSources = [
  { id: "paycom", label: "Paycom", order: 10 },
  { id: "amazon", label: "Amazon", order: 20 },
  { id: "slack", label: "Slack", order: 30 },
] as const;
export const personTitles = [
  { id: "owner", label: "Owner" },
  { id: "ops_manager", label: "OPs Manager" },
  { id: "fleet_manager", label: "Fleet Manager" },
  { id: "dispatch", label: "Dispatch" },
  { id: "hr_manager", label: "HR Manager" },
  { id: "manager", label: "Manager" },
] as const;
