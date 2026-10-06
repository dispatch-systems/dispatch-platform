// Generated from the features' read toggles.
// Run `npm run contracts:generate` after changing them.
export const readToggleGroups = [
  {
    "label": "Routes",
    "missing": "route data",
    "sources": [
      {
        "id": "routes",
        "label": "Routes"
      }
    ],
    "toggles": [
      {
        "id": "routes",
        "label": "Routes & packages",
        "hint": null,
        "missing": "routes",
        "source": "routes",
        "with": null,
        "optIn": false
      },
      {
        "id": "locations",
        "label": "Delivery addresses & GPS",
        "hint": "Stop addresses and GPS points",
        "missing": "delivery addresses",
        "source": "routes",
        "with": "routes",
        "optIn": true
      }
    ]
  },
  {
    "label": "Timecard",
    "missing": "timecard data",
    "sources": [
      {
        "id": "timecards",
        "label": "Timecard"
      },
      {
        "id": "meal_breaks",
        "label": "Meal Breaks"
      }
    ],
    "toggles": [
      {
        "id": "timecards",
        "label": "Timecards",
        "hint": null,
        "missing": "timecards",
        "source": "timecards",
        "with": null,
        "optIn": false
      },
      {
        "id": "meal_breaks",
        "label": "Meal breaks",
        "hint": null,
        "missing": "meal breaks",
        "source": "meal_breaks",
        "with": null,
        "optIn": false
      }
    ]
  },
  {
    "label": "DVIC",
    "missing": "DVIC inspections",
    "sources": [
      {
        "id": "dvic",
        "label": "DVIC"
      }
    ],
    "toggles": [
      {
        "id": "dvic",
        "label": "DVIC inspections",
        "hint": null,
        "missing": "DVIC inspections",
        "source": "dvic",
        "with": null,
        "optIn": false
      }
    ]
  },
  {
    "label": "Weekly Scorecard",
    "missing": "weekly scorecard data",
    "sources": [
      {
        "id": "weekly_scorecard",
        "label": "Weekly Scorecard"
      }
    ],
    "toggles": [
      {
        "id": "feedback",
        "label": "Customer feedback",
        "hint": null,
        "missing": "customer feedback",
        "source": "weekly_scorecard",
        "with": null,
        "optIn": false
      },
      {
        "id": "safety",
        "label": "Safety events",
        "hint": null,
        "missing": "safety events",
        "source": "weekly_scorecard",
        "with": null,
        "optIn": false
      },
      {
        "id": "returns",
        "label": "Returns & contact compliance",
        "hint": null,
        "missing": "returns",
        "source": "weekly_scorecard",
        "with": null,
        "optIn": false
      },
      {
        "id": "weekly_scorecard",
        "label": "Weekly Scorecard",
        "hint": null,
        "missing": "weekly scorecard",
        "source": "weekly_scorecard",
        "with": null,
        "optIn": false
      }
    ]
  },
  {
    "label": "Daily Performance",
    "missing": "daily performance data",
    "sources": [
      {
        "id": "daily_performance",
        "label": "Daily Performance"
      }
    ],
    "toggles": [
      {
        "id": "daily_performance",
        "label": "Daily Performance",
        "hint": null,
        "missing": "daily performance",
        "source": "daily_performance",
        "with": null,
        "optIn": false
      },
      {
        "id": "daily_feedback",
        "label": "Daily customer feedback",
        "hint": null,
        "missing": "daily customer feedback",
        "source": "daily_performance",
        "with": null,
        "optIn": false
      },
      {
        "id": "daily_returns",
        "label": "Daily returns & contact compliance",
        "hint": null,
        "missing": "daily returns & contact compliance",
        "source": "daily_performance",
        "with": null,
        "optIn": false
      },
      {
        "id": "daily_safety",
        "label": "Daily safety events",
        "hint": null,
        "missing": "daily safety events",
        "source": "daily_performance",
        "with": null,
        "optIn": false
      }
    ]
  }
] as const;
