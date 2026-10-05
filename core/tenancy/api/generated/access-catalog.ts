// Generated from backend feature, collector and permission catalogs.
// Run `npm run contracts:generate` after changing them.
export const pages = [
  "timecard",
  "uniforms",
  "routes",
  "dvic",
  "weekly_scorecard",
  "driver_match"
] as const;
export const pageTabs = [
  "timecard.daily",
  "timecard.meal_breaks",
  "timecard.employees",
  "dvic.day",
  "dvic.week"
] as const;
export const connections = [
  "paycom",
  "cortex"
] as const;
export const features = [
  "timecard",
  "uniforms",
  "routes",
  "dvic",
  "weekly_scorecard",
  "driver_match",
  "timecard.daily",
  "timecard.meal_breaks",
  "timecard.employees",
  "dvic.day",
  "dvic.week",
  "paycom",
  "cortex"
] as const;
export const featureCatalog = [
  {
    "id": "timecard",
    "label": "Timecard",
    "permissions": [
      "timecard.view",
      "timecard.manage",
      "collections.run"
    ],
    "requires": [
      "timecards",
      "meal_breaks"
    ],
    "kind": "page"
  },
  {
    "id": "uniforms",
    "label": "Uniform Inventory",
    "permissions": [
      "uniforms.view",
      "uniforms.adjust",
      "uniforms.manage"
    ],
    "requires": [],
    "kind": "page"
  },
  {
    "id": "routes",
    "label": "Routes",
    "permissions": [
      "routes.view",
      "routes.collect",
      "routes.manage"
    ],
    "requires": [
      "routes"
    ],
    "kind": "page"
  },
  {
    "id": "dvic",
    "label": "DVIC",
    "permissions": [
      "dvic.view",
      "dvic.collect",
      "dvic.manage"
    ],
    "requires": [
      "dvic"
    ],
    "kind": "page"
  },
  {
    "id": "weekly_scorecard",
    "label": "Weekly Scorecard",
    "permissions": [
      "weekly_scorecard.view",
      "weekly_scorecard.collect",
      "weekly_scorecard.manage"
    ],
    "requires": [
      "weekly_scorecard"
    ],
    "kind": "page"
  },
  {
    "id": "driver_match",
    "label": "Driver Match",
    "permissions": [
      "driver_match.manage"
    ],
    "requires": [
      "timecards",
      "routes"
    ],
    "kind": "page"
  },
  {
    "id": "timecard.daily",
    "label": "Timecard",
    "permissions": [],
    "requires": [],
    "kind": "tab",
    "page": "timecard"
  },
  {
    "id": "timecard.meal_breaks",
    "label": "Meal Breaks",
    "permissions": [],
    "requires": [],
    "kind": "tab",
    "page": "timecard"
  },
  {
    "id": "timecard.employees",
    "label": "Employee Search",
    "permissions": [],
    "requires": [],
    "kind": "tab",
    "page": "timecard"
  },
  {
    "id": "dvic.day",
    "label": "Day",
    "permissions": [],
    "requires": [],
    "kind": "tab",
    "page": "dvic"
  },
  {
    "id": "dvic.week",
    "label": "Week",
    "permissions": [],
    "requires": [],
    "kind": "tab",
    "page": "dvic"
  },
  {
    "id": "paycom",
    "label": "Paycom",
    "permissions": [],
    "requires": [],
    "kind": "connection",
    "provides": [
      "timecards"
    ]
  },
  {
    "id": "cortex",
    "label": "Cortex",
    "permissions": [],
    "requires": [],
    "kind": "connection",
    "provides": [
      "meal_breaks",
      "routes",
      "dvic",
      "weekly_scorecard"
    ]
  }
] as const;
export const schedulesFeature = "timecard" as const;
export const permissions = [
  "uniforms.view",
  "uniforms.adjust",
  "uniforms.manage",
  "timecard.view",
  "timecard.manage",
  "collections.run",
  "routes.view",
  "routes.collect",
  "routes.manage",
  "dvic.view",
  "dvic.collect",
  "dvic.manage",
  "weekly_scorecard.view",
  "weekly_scorecard.collect",
  "weekly_scorecard.manage",
  "driver_match.manage",
  "connections.manage",
  "members.invite",
  "members.manage",
  "roles.manage",
  "settings.manage"
] as const;
export const permissionLabels = {
  "collections.run": "Run Collections",
  "connections.manage": "Manage Connections",
  "driver_match.manage": "Manage Driver Match",
  "dvic.collect": "Collect DVIC",
  "dvic.manage": "Manage DVIC",
  "dvic.view": "View DVIC",
  "members.invite": "Invite Members",
  "members.manage": "Manage Members",
  "roles.manage": "Manage Roles",
  "routes.collect": "Collect Routes",
  "routes.manage": "Manage Routes",
  "routes.view": "View Routes",
  "settings.manage": "Manage DSP Settings",
  "timecard.manage": "Manage Timecard",
  "timecard.view": "View Timecard",
  "uniforms.adjust": "Adjust Uniform Inventory",
  "uniforms.manage": "Manage Uniform Inventory",
  "uniforms.view": "View Uniform Inventory",
  "weekly_scorecard.collect": "Collect Weekly Scorecard",
  "weekly_scorecard.manage": "Manage Weekly Scorecard",
  "weekly_scorecard.view": "View Weekly Scorecard"
} as const;
export const permissionGroups = [
  [
    "Timecard",
    [
      "timecard.view",
      "timecard.manage",
      "collections.run"
    ]
  ],
  [
    "Uniform Inventory",
    [
      "uniforms.view",
      "uniforms.adjust",
      "uniforms.manage"
    ]
  ],
  [
    "Routes",
    [
      "routes.view",
      "routes.collect",
      "routes.manage"
    ]
  ],
  [
    "DVIC",
    [
      "dvic.view",
      "dvic.collect",
      "dvic.manage"
    ]
  ],
  [
    "Weekly Scorecard",
    [
      "weekly_scorecard.view",
      "weekly_scorecard.collect",
      "weekly_scorecard.manage"
    ]
  ],
  [
    "Driver Match",
    [
      "driver_match.manage"
    ]
  ],
  [
    "Connections",
    [
      "connections.manage"
    ]
  ],
  [
    "Team",
    [
      "members.invite",
      "members.manage",
      "roles.manage"
    ]
  ],
  [
    "DSP",
    [
      "settings.manage"
    ]
  ]
] as const;
export const impliedPermissions = {
  "dvic.collect": "dvic.view",
  "dvic.manage": "dvic.view",
  "routes.collect": "routes.view",
  "routes.manage": "routes.view",
  "timecard.manage": "timecard.view",
  "uniforms.adjust": "uniforms.view",
  "uniforms.manage": "uniforms.view",
  "weekly_scorecard.collect": "weekly_scorecard.view",
  "weekly_scorecard.manage": "weekly_scorecard.view"
} as const;
