// Generated from backend feature, collector and permission catalogs.
// Run `npm run contracts:generate` after changing them.
export const pages = [
  "home",
  "timecard",
  "uniforms",
  "routes",
  "dvic",
  "weekly_scorecard",
  "driver_profiles",
  "driver_match",
  "daily_performance",
  "documents",
  "communications",
  "team",
  "settings",
  "slack",
  "vans"
] as const;
export const subfeatures = [
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
  "home",
  "timecard",
  "uniforms",
  "routes",
  "dvic",
  "weekly_scorecard",
  "driver_profiles",
  "driver_match",
  "daily_performance",
  "documents",
  "communications",
  "team",
  "settings",
  "slack",
  "vans",
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
    "id": "home",
    "label": "Home Page",
    "permissions": [],
    "requires": [],
    "kind": "page",
    "mandatory": true
  },
  {
    "id": "timecard",
    "label": "Timecard",
    "permissions": [
      "timecard.view",
      "timecard.manage",
      "collections.run"
    ],
    "requires": [
      "timecards"
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
    "id": "driver_profiles",
    "label": "Driver Profiles",
    "permissions": [
      "driver_profiles.collect"
    ],
    "requires": [
      "associates"
    ],
    "kind": "page"
  },
  {
    "id": "driver_match",
    "label": "Driver Match",
    "permissions": [
      "driver_match.manage"
    ],
    "requires": [],
    "kind": "page",
    "mandatory": true
  },
  {
    "id": "daily_performance",
    "label": "Daily Performance",
    "permissions": [
      "daily_performance.view",
      "daily_performance.collect",
      "daily_performance.manage"
    ],
    "requires": [
      "daily_performance"
    ],
    "kind": "page"
  },
  {
    "id": "documents",
    "label": "Documents",
    "permissions": [
      "documents.use"
    ],
    "requires": [],
    "kind": "page",
    "connects": true
  },
  {
    "id": "communications",
    "label": "Communications",
    "permissions": [
      "communications.use"
    ],
    "requires": [],
    "kind": "page"
  },
  {
    "id": "team",
    "label": "Team & Roles",
    "permissions": [],
    "requires": [],
    "kind": "page",
    "mandatory": true
  },
  {
    "id": "settings",
    "label": "Settings",
    "permissions": [],
    "requires": [],
    "kind": "page",
    "mandatory": true
  },
  {
    "id": "slack",
    "label": "Slack",
    "permissions": [
      "slack.manage"
    ],
    "requires": [],
    "kind": "page",
    "connects": true
  },
  {
    "id": "vans",
    "label": "Vans",
    "permissions": [
      "vans.collect"
    ],
    "requires": [
      "vans"
    ],
    "kind": "page"
  },
  {
    "id": "timecard.daily",
    "label": "Timecard",
    "permissions": [],
    "requires": [],
    "kind": "sub",
    "page": "timecard",
    "tab": true
  },
  {
    "id": "timecard.meal_breaks",
    "label": "Meal Breaks",
    "permissions": [],
    "requires": [
      "meal_breaks"
    ],
    "kind": "sub",
    "page": "timecard",
    "tab": true
  },
  {
    "id": "timecard.employees",
    "label": "Employee Search",
    "permissions": [],
    "requires": [],
    "kind": "sub",
    "page": "timecard",
    "tab": true
  },
  {
    "id": "dvic.day",
    "label": "Day",
    "permissions": [],
    "requires": [],
    "kind": "sub",
    "page": "dvic",
    "tab": true
  },
  {
    "id": "dvic.week",
    "label": "Week",
    "permissions": [],
    "requires": [],
    "kind": "sub",
    "page": "dvic",
    "tab": true
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
      "daily_performance",
      "weekly_scorecard",
      "associates",
      "vans"
    ]
  }
] as const;
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
  "driver_profiles.collect",
  "driver_match.manage",
  "members.invite",
  "members.manage",
  "roles.manage",
  "settings.manage",
  "connections.manage",
  "communications.use",
  "daily_performance.view",
  "daily_performance.collect",
  "daily_performance.manage",
  "documents.use",
  "slack.manage",
  "vans.collect"
] as const;
export const permissionLabels = {
  "collections.run": "Run Collections",
  "communications.use": "Use Communications",
  "connections.manage": "Manage DSP Connections",
  "daily_performance.collect": "Collect Daily Performance",
  "daily_performance.manage": "Manage Daily Performance",
  "daily_performance.view": "View Daily Performance",
  "documents.use": "Use Documents",
  "driver_match.manage": "Manage Driver Match",
  "driver_profiles.collect": "Collect Driver Profiles",
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
  "slack.manage": "Manage Slack",
  "timecard.manage": "Manage Timecard",
  "timecard.view": "View Timecard",
  "uniforms.adjust": "Adjust Uniform Inventory",
  "uniforms.manage": "Manage Uniform Inventory",
  "uniforms.view": "View Uniform Inventory",
  "vans.collect": "Collect Vans",
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
    "Driver Profiles",
    [
      "driver_profiles.collect"
    ]
  ],
  [
    "Driver Match",
    [
      "driver_match.manage"
    ]
  ],
  [
    "Daily Performance",
    [
      "daily_performance.view",
      "daily_performance.collect",
      "daily_performance.manage"
    ]
  ],
  [
    "Documents",
    [
      "documents.use"
    ]
  ],
  [
    "Communications",
    [
      "communications.use"
    ]
  ],
  [
    "Slack",
    [
      "slack.manage"
    ]
  ],
  [
    "Vans",
    [
      "vans.collect"
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
    "DSP Settings",
    [
      "settings.manage",
      "connections.manage"
    ]
  ]
] as const;
export const impliedPermissions = {
  "daily_performance.collect": [
    "daily_performance.view"
  ],
  "daily_performance.manage": [
    "daily_performance.view"
  ],
  "dvic.collect": [
    "dvic.view"
  ],
  "dvic.manage": [
    "dvic.view"
  ],
  "routes.collect": [
    "routes.view"
  ],
  "routes.manage": [
    "routes.view"
  ],
  "timecard.manage": [
    "timecard.view"
  ],
  "uniforms.adjust": [
    "uniforms.view"
  ],
  "uniforms.manage": [
    "uniforms.view"
  ],
  "weekly_scorecard.collect": [
    "weekly_scorecard.view"
  ],
  "weekly_scorecard.manage": [
    "weekly_scorecard.view"
  ]
} as const;
export const permissionParents = {
  "uniforms.adjust": "uniforms.view",
  "uniforms.manage": "uniforms.view"
} as const;
export const schedulingPermissions = [
  "daily_performance.manage",
  "driver_profiles.collect",
  "dvic.manage",
  "routes.manage",
  "timecard.manage",
  "vans.collect",
  "weekly_scorecard.manage"
] as const;
