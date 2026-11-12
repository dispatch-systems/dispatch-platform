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
  "vans",
  "live_route_data",
  "conduct"
] as const;
export const subfeatures = [
  "timecard.daily",
  "timecard.meal_breaks",
  "timecard.employees",
  "dvic.day",
  "dvic.week",
  "communications.gifs"
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
  "live_route_data",
  "conduct",
  "timecard.daily",
  "timecard.meal_breaks",
  "timecard.employees",
  "dvic.day",
  "dvic.week",
  "communications.gifs",
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
    "place": 10,
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
    "kind": "page",
    "place": 20
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
    "kind": "page",
    "place": 30
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
    "kind": "page",
    "place": 40
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
    "kind": "page",
    "place": 50
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
    "kind": "page",
    "place": 60
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
    "kind": "page",
    "place": 70
  },
  {
    "id": "driver_match",
    "label": "Driver Match",
    "permissions": [
      "driver_match.manage"
    ],
    "requires": [],
    "kind": "page",
    "place": 75,
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
    "kind": "page",
    "place": 80
  },
  {
    "id": "documents",
    "label": "Documents",
    "permissions": [
      "documents.use"
    ],
    "requires": [],
    "kind": "page",
    "place": 90,
    "connects": true
  },
  {
    "id": "communications",
    "label": "Dispatch Phone",
    "permissions": [
      "communications.use",
      "phones.manage"
    ],
    "requires": [],
    "kind": "page",
    "place": 95,
    "connects": true
  },
  {
    "id": "team",
    "label": "Team & Roles",
    "permissions": [],
    "requires": [],
    "kind": "page",
    "place": 100,
    "mandatory": true
  },
  {
    "id": "settings",
    "label": "Settings",
    "permissions": [],
    "requires": [],
    "kind": "page",
    "place": 110,
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
    "place": 120,
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
    "kind": "page",
    "place": 130
  },
  {
    "id": "live_route_data",
    "label": "Live Route Data",
    "permissions": [
      "live_route_data.manage"
    ],
    "requires": [
      "routes"
    ],
    "kind": "page",
    "place": 140
  },
  {
    "id": "conduct",
    "label": "Conduct",
    "permissions": [
      "conduct.use",
      "conduct.manage"
    ],
    "requires": [],
    "kind": "page",
    "place": 150
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
    "id": "communications.gifs",
    "label": "GIFs",
    "permissions": [],
    "requires": [],
    "kind": "sub",
    "page": "communications",
    "tab": false
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
  "collections.run",
  "communications.use",
  "conduct.manage",
  "conduct.use",
  "connections.manage",
  "daily_performance.collect",
  "daily_performance.manage",
  "daily_performance.view",
  "documents.use",
  "driver_match.manage",
  "driver_profiles.collect",
  "dvic.collect",
  "dvic.manage",
  "dvic.view",
  "live_route_data.manage",
  "members.invite",
  "members.manage",
  "phones.manage",
  "roles.manage",
  "routes.collect",
  "routes.manage",
  "routes.view",
  "settings.manage",
  "slack.manage",
  "timecard.manage",
  "timecard.view",
  "uniforms.adjust",
  "uniforms.manage",
  "uniforms.view",
  "vans.collect",
  "weekly_scorecard.collect",
  "weekly_scorecard.manage",
  "weekly_scorecard.view"
] as const;
export const permissionLabels = {
  "collections.run": "Run Collections",
  "communications.use": "Use Dispatch Phone",
  "conduct.manage": "Manage Conduct",
  "conduct.use": "Use Conduct",
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
  "live_route_data.manage": "Manage Live Route Data",
  "members.invite": "Invite Members",
  "members.manage": "Manage Members",
  "phones.manage": "Manage Phones",
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
    "Dispatch Phone",
    [
      "communications.use",
      "phones.manage"
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
    "Live Route Data",
    [
      "live_route_data.manage"
    ]
  ],
  [
    "Conduct",
    [
      "conduct.use",
      "conduct.manage"
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
  "conduct.manage": [
    "conduct.use"
  ],
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
