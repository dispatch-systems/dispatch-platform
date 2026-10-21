// Generated from the kinds of data the features' people declare, their sources and the titles a person may be given.
// Run `npm run contracts:generate` after changing them.
import { Award, CalendarDays, ClipboardCheck, Contact, MessageSquare, Route, Utensils } from 'lucide-react';
export const peopleKinds = [
  { id: "timecards", label: "Timecards", unit: "day", icon: CalendarDays },
  { id: "driver_profiles", label: "Roster", unit: null, icon: Contact },
  { id: "routes", label: "Routes", unit: "day", icon: Route },
  { id: "meal_breaks", label: "Meal breaks", unit: "day", icon: Utensils },
  { id: "weekly_scorecard", label: "Weekly Scorecard", unit: "week", icon: Award },
  { id: "daily_performance", label: "Daily Performance", unit: "day", icon: CalendarDays },
  { id: "slack", label: "Slack", unit: null, icon: MessageSquare },
  { id: "dvic", label: "DVIC", unit: "inspection", icon: ClipboardCheck },
] as const;
export const peopleSources = [
  { id: "paycom", label: "Paycom" },
  { id: "amazon", label: "Amazon" },
  { id: "slack", label: "Slack" },
] as const;
export const personTitles = [
  { id: "owner", label: "Owner" },
  { id: "ops_manager", label: "OPs Manager" },
  { id: "fleet_manager", label: "Fleet Manager" },
  { id: "dispatch", label: "Dispatch" },
  { id: "hr_manager", label: "HR Manager" },
  { id: "manager", label: "Manager" },
] as const;
