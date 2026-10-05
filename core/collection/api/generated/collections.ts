// Generated from the collectors' collections.
// Run `npm run contracts:generate` after changing them.
export const collections = [
  {
    "kind": "paycom.collect",
    "provider": "paycom",
    "schedule": "paycom",
    "label": "Paycom",
    "unit": "employee",
    "counted": "employees"
  },
  {
    "kind": "cortex.meal_breaks.collect",
    "provider": "cortex",
    "schedule": "meal_break",
    "label": "Meal breaks",
    "unit": "itinerary",
    "counted": "itineraries"
  },
  {
    "kind": "cortex.weekly_scorecard.collect",
    "provider": "cortex",
    "schedule": "weekly_scorecard",
    "label": "Weekly Scorecard",
    "unit": "row",
    "counted": "rows"
  },
  {
    "kind": "cortex.routes.collect",
    "provider": "cortex",
    "schedule": "routes",
    "label": "Routes",
    "unit": "itinerary",
    "counted": "itineraries"
  },
  {
    "kind": "cortex.dvic.collect",
    "provider": "cortex",
    "schedule": "dvic",
    "label": "DVIC",
    "unit": "row",
    "counted": "rows"
  }
] as const;
