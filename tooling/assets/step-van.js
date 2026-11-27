import * as THREE from 'three';
import { amazonDecal, createKit, finish, primeDecal, roundedOutline } from './kit.js';

// Original stylized mesh authored for Dispatch from real vehicle photographs.
// X = length (front is negative X), Y = up, Z = width.
// +Z is the driver side; -Z is the passenger side. Units approximately metres.
export function createStepVan() {
  const paints = {
    body: new THREE.MeshStandardMaterial({ color: '#425e6a', roughness: 0.7, metalness: 0.12 }),
    trim: new THREE.MeshStandardMaterial({ color: '#9aa8af', roughness: 0.45, metalness: 0.55 }),
    dark: new THREE.MeshStandardMaterial({ color: '#18222b', roughness: 0.85 }),
    rubber: new THREE.MeshStandardMaterial({ color: '#171a20', roughness: 0.95 }),
    glass: new THREE.MeshStandardMaterial({ color: '#263e43', roughness: 0.3, metalness: 0.15 }),
    reflection: new THREE.MeshStandardMaterial({
      color: '#66817e',
      roughness: 0.65,
      metalness: 0.1,
    }),
    light: new THREE.MeshStandardMaterial({
      color: '#eef5ed',
      roughness: 0.35,
      emissive: '#dceaf0',
      emissiveIntensity: 0.18,
    }),
    amber: new THREE.MeshStandardMaterial({
      color: '#ffa23b',
      roughness: 0.5,
      emissive: '#ff8217',
      emissiveIntensity: 0.2,
    }),
    red: new THREE.MeshStandardMaterial({
      color: '#d93f43',
      roughness: 0.5,
      emissive: '#be1c28',
      emissiveIntensity: 0.15,
    }),
    white: new THREE.MeshStandardMaterial({ color: '#e8edf3', roughness: 0.7 }),
  };
  const { van, part, box, cylinder, tube, polygon, skin, outline, mappedSkin, mappedOutline } =
    createKit('Dispatch_Step_Van', paints);
  // These outlines follow the passenger and driver photographs supplied by the user.
  // Both doors have split upper glazing. Only the passenger door has the lower sight window.
  const frontAxle = -3.05,
    rearAxle = 1.52;
  const quarterPoints = [
    [-3.22, 1.8],
    [-2.77, 2.64],
    [-2.61, 2.64],
    [-2.61, 1.8],
  ];
  const upperPoints = [
    [-2.5, 1.81],
    [-2.5, 2.6],
    [-1.66, 2.6],
    [-1.66, 1.81],
  ];
  const lowerPoints = [
    [-2.39, 1.51],
    [-1.71, 1.51],
    [-1.71, 0.91],
    [-2.25, 0.91],
  ];
  const quarterShape = roundedOutline(quarterPoints, 0.025);
  const upperShape = roundedOutline(upperPoints, 0.075);
  const lowerShape = roundedOutline(lowerPoints, 0.07);
  function endLight(x, y, z, r, mat) {
    const mesh = part(new THREE.CylinderGeometry(r, r, 0.028, 24), mat, x, y, z);
    mesh.rotation.z = Math.PI / 2;
    return mesh;
  }
  function doorOutline(passenger) {
    return roundedOutline(
      passenger
        ? [
            [-2.57, 2.69],
            [-1.54, 2.69],
            [-1.54, 0.43],
            [-2.28, 0.43],
            [-2.57, 1.78],
          ]
        : [
            [-2.57, 2.69],
            [-1.54, 2.69],
            [-1.54, 0.43],
            [-2.44, 0.43],
            [-2.57, 1.13],
          ],
      0.015,
    );
  }
  function sidePanel(passenger) {
    const shape = new THREE.Shape();
    shape.moveTo(-4.02, 0.82);
    shape.lineTo(-4.02, 1.48);
    shape.lineTo(-3.34, 1.72);
    shape.lineTo(-2.73, 2.76);
    shape.lineTo(-2.43, 3.24);
    shape.lineTo(3.82, 3.24);
    shape.lineTo(3.82, 0.43);
    shape.lineTo(rearAxle + 0.6, 0.43);
    shape.absarc(rearAxle, 0.43, 0.6, 0, Math.PI, false);
    shape.lineTo(frontAxle + 0.6, 0.43);
    shape.absarc(frontAxle, 0.43, 0.6, 0, Math.PI, false);
    shape.lineTo(-4.02, 0.82);
    shape.holes.push(quarterShape.clone(), upperShape.clone());
    if (passenger) shape.holes.push(lowerShape.clone());
    return new THREE.ExtrudeGeometry(shape, {
      depth: 0.07,
      bevelEnabled: false,
      curveSegments: 28,
    });
  }
  part(sidePanel(false), 'body', 0, 0, 1.16, 'Driver side, solid lower door');
  part(sidePanel(true), 'body', 0, 0, -1.23, 'Passenger side, lower sight window');
  // Cargo shell: use separate panels so the wheel openings remain real openings.
  box(5.3, 0.07, 2.38, 1.15, 3.24, 0, 'body', 'Cargo roof');
  box(0.08, 2.8, 2.4, 3.8, 1.83, 0, 'body', 'Rear body');
  box(5.3, 0.12, 1.9, 1.15, 0.49, 0, 'dark', 'Cargo floor');
  box(0.06, 2.72, 2.34, -1.48, 1.86, 0, 'body', 'Cab bulkhead');
  box(0.95, 0.075, 2.37, -1.955, 3.24, 0, 'body', 'Cab roof');
  // Sloping forehead, paired windshield and sculpted hood.
  polygon(
    [
      [-2.45, 3.27, -1.2],
      [-2.45, 3.27, 1.2],
      [-2.76, 2.75, 1.2],
      [-2.76, 2.75, -1.2],
    ],
    'body',
  );
  polygon(
    [
      [-3.34, 1.74, -1.18],
      [-3.34, 1.74, 1.18],
      [-2.75, 2.76, 1.18],
      [-2.75, 2.76, -1.18],
    ],
    'dark',
  );
  const windshieldMap = (u, v) => [-3.34 + (v - 1.74) * (0.59 / 1.02) - 0.018, v, u];
  for (const sign of [-1, 1]) {
    const left = sign < 0 ? -1.095 : 0.055,
      right = sign < 0 ? -0.055 : 1.095;
    const pane = roundedOutline(
      [
        [left, 1.805],
        [left, 2.695],
        [right, 2.695],
        [right, 1.805],
      ],
      0.105,
    );
    mappedSkin(pane, 'glass', windshieldMap, 'Rounded windshield');
    mappedOutline(pane, windshieldMap, 0.022, 'dark');
    const streak = roundedOutline(
      [
        [left + 0.09, 1.88],
        [left + 0.23, 2.61],
        [left + 0.26, 2.61],
        [left + 0.12, 1.88],
      ],
      0.01,
    );
    mappedSkin(
      streak,
      'reflection',
      (u, v) => {
        const p = windshieldMap(u, v);
        p[0] -= 0.006;
        return p;
      },
      'Windshield reflection',
    );
    // The reference has wiper pivots above the panes, with arms resting along their tops.
    const z = sign * 0.8;
    tube(
      [
        [-2.745, 2.82, z],
        [-2.855, 2.66, sign * 0.49],
      ],
      0.013,
      'dark',
      1,
    );
    tube(
      [
        [-2.85, 2.665, sign * 0.12],
        [-2.86, 2.65, sign * 0.85],
      ],
      0.018,
      'dark',
      1,
    );
  }
  tube(
    [
      [-3.37, 1.75, 0],
      [-2.78, 2.76, 0],
    ],
    0.025,
    'body',
    1,
  );
  tube(
    [
      [-3.36, 1.76, -1.17],
      [-2.77, 2.76, -1.17],
    ],
    0.032,
    'body',
    1,
  );
  tube(
    [
      [-3.36, 1.76, 1.17],
      [-2.77, 2.76, 1.17],
    ],
    0.032,
    'body',
    1,
  );
  polygon(
    [
      [-4.02, 1.49, -1.16],
      [-4.02, 1.49, 1.16],
      [-3.35, 1.75, 1.16],
      [-3.35, 1.75, -1.16],
    ],
    'body',
  );
  box(0.085, 0.66, 2.31, -4.035, 1.16, 0, 'body', 'Nose');
  const grille = roundedOutline(
    [
      [-0.64, 0.88],
      [-0.67, 1.32],
      [-0.48, 1.4],
      [0, 1.47],
      [0.48, 1.4],
      [0.67, 1.32],
      [0.64, 0.88],
    ],
    0.06,
  );
  mappedSkin(grille, 'dark', (u, v) => [-4.092, v, u], 'Arched grille recess');
  for (let i = 0; i < 15; i++)
    box(
      0.01,
      0.008,
      i > 11 ? 1.02 : 1.26,
      -4.102,
      0.91 + i * 0.034,
      0,
      'trim',
      'Fine grille horizontal bars',
    );
  for (let i = -4; i <= 4; i++)
    box(0.012, 0.47, 0.008, -4.106, 1.155, i * 0.134, 'trim', 'Grille vertical bars');
  box(0.027, 0.045, 0.22, -4.122, 1.45, 0, 'trim', 'Grille badge');
  box(0.23, 0.2, 2.6, -4.13, 0.68, 0, 'dark', 'Front bumper');
  box(0.045, 0.027, 2.4, -4.255, 0.786, 0, 'trim', 'Bumper edge');
  box(0.012, 0.091, 0.8, -4.251, 0.665, 0, 'rubber', 'Bumper centre inset');
  for (const z of [-0.97, 0.97]) {
    const housing = roundedOutline(
      [
        [z - 0.19, 1.08],
        [z - 0.19, 1.43],
        [z + 0.19, 1.43],
        [z + 0.19, 1.08],
      ],
      0.06,
    );
    mappedSkin(housing, 'trim', (u, v) => [-4.1, v, u], 'Chrome headlight surround');
    const light = roundedOutline(
      [
        [z - 0.139, 1.13],
        [z - 0.139, 1.386],
        [z + 0.139, 1.386],
        [z + 0.139, 1.13],
      ],
      0.045,
    );
    mappedSkin(light, 'light', (u, v) => [-4.114, v, u], 'Rounded rectangular headlight');
    mappedOutline(light, (u, v) => [-4.12, v, u], 0.012, 'dark');
    const indicator = roundedOutline(
      [
        [z - 0.14, 0.926],
        [z - 0.14, 1.022],
        [z + 0.14, 1.022],
        [z + 0.14, 0.926],
      ],
      0.04,
    );
    mappedSkin(indicator, 'amber', (u, v) => [-4.115, v, u], 'Rounded amber indicator');
    mappedOutline(indicator, (u, v) => [-4.12, v, u], 0.011, 'dark');
  }
  box(0.025, 0.13, 0.27, -4.266, 0.68, -0.91, 'white', 'Front plate');
  // The entry doors are different on the two sides, as in the supplied photographs.
  for (const sign of [-1, 1]) {
    const z = sign * 1.244,
      passenger = sign < 0;
    const door = doorOutline(passenger);
    door.holes.push(upperShape.clone());
    if (passenger) door.holes.push(lowerShape.clone());
    skin(door, z, 'body', passenger ? 'Passenger sliding door' : 'Driver sliding door');
    outline(door, z + sign * 0.013, 0.011, 'dark');
    skin(quarterShape, z, 'glass', 'Fixed quarter window');
    outline(quarterShape, z + sign * 0.02, 0.025);
    skin(upperShape, z + sign * 0.008, 'glass', 'Split sliding upper glass');
    outline(upperShape, z + sign * 0.023, 0.027);
    box(0.025, 0.735, 0.024, -2.075, 2.205, z + sign * 0.035, 'dark', 'Sliding window divider');
    // Small, restrained reflections inside the glazing rather than bright opaque panes.
    polygon(
      [
        [-2.44, 2.51, z + sign * 0.012],
        [-2.39, 2.54, z + sign * 0.012],
        [-2.15, 1.91, z + sign * 0.012],
        [-2.2, 1.91, z + sign * 0.012],
      ],
      'reflection',
    );
    polygon(
      [
        [-2.68, 2.5, z + sign * 0.012],
        [-2.65, 2.54, z + sign * 0.012],
        [-2.65, 1.91, z + sign * 0.012],
        [-2.7, 1.91, z + sign * 0.012],
      ],
      'reflection',
    );
    if (passenger) {
      skin(lowerShape, z + sign * 0.008, 'glass', 'Passenger lower sight window');
      outline(lowerShape, z + sign * 0.023, 0.026);
      polygon(
        [
          [-2.27, 1.44, z + sign * 0.012],
          [-2.23, 1.44, z + sign * 0.012],
          [-2.1, 0.99, z + sign * 0.012],
          [-2.14, 0.99, z + sign * 0.012],
        ],
        'reflection',
      );
    }
    box(0.044, 0.17, 0.05, -2.525, 1.67, z + sign * 0.027, 'dark', 'Door handle');
    box(1.1, 0.045, 0.065, -2.06, 2.71, z + sign * 0.015, 'dark', 'Sliding door upper track');
    box(0.27, 0.058, 0.075, -2.05, 2.98, z + sign * 0.01, 'trim', 'Door header vent');
    box(1.02, 0.055, 0.24, -2.02, 0.425, sign * 1.17, 'dark', 'Entry step');
    box(0.99, 0.024, 0.035, -2.02, 0.46, sign * 1.3, 'trim', 'Step edge');
    for (const y of [1.6, 1.66]) box(5.18, 0.027, 0.035, 1.16, y, z, 'trim', 'Cargo belt rail');
    for (const [x, len] of [
      [-0.29, 2.27],
      [2.96, 1.55],
    ])
      box(len, 0.035, 0.04, x, 0.48, z, 'trim', 'Lower rail');
    box(0.055, 2.78, 0.045, -1.48, 1.83, z, 'trim', 'Cargo front seam');
    box(0.055, 2.78, 0.045, 3.76, 1.83, z, 'trim', 'Cargo rear seam');
    box(6.18, 0.055, 0.06, 0.66, 3.25, z, 'trim', 'Roof edge');
    // Two stacked mirrors, mounted just forward of the entry door.
    tube(
      [
        [-2.76, 1.79, z],
        [-2.75, 1.82, sign * 1.51],
        [-2.7, 2.71, sign * 1.51],
        [-2.72, 2.76, z],
      ],
      0.019,
      'dark',
      12,
    );
    box(0.16, 0.36, 0.105, -2.71, 2.42, sign * 1.53, 'dark', 'Mirror housing');
    box(0.12, 0.3, 0.018, -2.71, 2.42, sign * 1.59, 'trim', 'Mirror glass');
    box(0.16, 0.17, 0.105, -2.74, 2.0, sign * 1.53, 'dark', 'Convex mirror');
    box(0.12, 0.13, 0.018, -2.74, 2.0, sign * 1.59, 'trim');
    box(0.125, 0.28, 0.021, -3.23, 1.38, z, 'dark', 'Engine side vent');
    for (let j = 0; j < 7; j++)
      box(0.1, 0.011, 0.025, -3.23, 1.27 + j * 0.036, z + sign * 0.013, 'body');
    for (const [x, y] of [
      [-3.86, 1.0],
      [-0.12, 0.68],
    ])
      cylinder(0.043, 0.025, x, y, z + sign * 0.012, 'amber', 16);
    cylinder(0.044, 0.025, 3.62, 0.58, z + sign * 0.012, 'red', 16);
    for (const x of [frontAxle, rearAxle]) {
      const curve = [];
      for (let j = 0; j <= 28; j++) {
        const a = (Math.PI * j) / 28;
        curve.push([x + Math.cos(a) * 0.6, 0.43 + Math.sin(a) * 0.6, z]);
      }
      tube(curve, 0.023, 'body', 28);
    }
    for (let i = 0; i < 21; i++)
      for (const y of [0.66, 3.12]) {
        const x = -1.28 + i * 0.25;
        if (y > 0.7 || Math.abs(x - rearAxle) > 0.6) cylinder(0.009, 0.015, x, y, z, 'trim', 6);
      }
    if (!passenger) {
      box(0.25, 0.25, 0.024, 2.38, 0.91, z, 'dark', 'Driver side fuel filler recess');
      cylinder(0.039, 0.037, 2.38, 0.91, z + 0.025, 'amber', 12);
    }
  }
  // Clearance lights, roof vent and the rear face from the supplied rear reference.
  for (const z of [-0.95, -0.3, 0, 0.3, 0.95]) {
    box(0.045, 0.03, 0.05, -2.5, 3.16, z, 'amber', 'Roof clearance light');
  }
  const roofVent = part(
    new THREE.CylinderGeometry(0.07, 0.07, 0.07, 16),
    'dark',
    0.82,
    3.32,
    0,
    'Roof vent',
  );
  box(0.22, 0.025, 0.22, 0.82, 3.36, 0, 'dark', 'Roof vent cap');
  // Closed four-section roll-up door. The open gap in the photo is not bodywork.
  box(0.025, 2.51, 1.62, 3.86, 1.77, 0, 'dark', 'Rear door recessed opening');
  box(0.028, 2.3, 1.56, 3.887, 1.76, 0, 'body', 'Rear roll-up door');
  for (const y of [0.61, 1.185, 1.76, 2.335, 2.91])
    box(0.012, 0.01, 1.55, 3.907, y, 0, 'trim', 'Roll-up panel seam');
  for (const z of [-0.82, 0.82]) box(0.032, 2.57, 0.023, 3.914, 1.76, z, 'trim', 'Rear door track');
  box(0.025, 0.035, 1.67, 3.914, 3.035, 0, 'trim', 'Rear door header');
  for (const z of [-0.96, 0.96]) {
    tube(
      [
        [3.94, 1.49, z],
        [3.99, 1.53, z],
        [3.99, 2.2, z],
        [3.94, 2.24, z],
      ],
      0.015,
      'trim',
      8,
    );
  }
  box(0.038, 0.026, 0.18, 3.938, 0.83, 0, 'dark', 'Roll-up pull handle');
  box(0.019, 0.095, 0.032, 3.93, 1.28, 0, 'trim', 'Rear central latch');
  box(0.04, 0.15, 0.023, 3.943, 1.21, 0.018, 'dark', 'Rear latch lever');
  for (const y of [0.8, 1.3, 1.87, 2.42, 2.78])
    for (const z of [-0.63, -0.26, 0.26, 0.63]) endLight(3.912, y, z, 0.008, 'trim');
  box(0.025, 0.025, 2.1, 3.905, 3.17, 0, 'dark', 'Rear rain gutter');
  box(0.095, 0.045, 0.1, 3.938, 3.115, 0, 'dark', 'Rear camera');
  box(0.4, 0.13, 2.5, 3.98, 0.37, 0, 'dark', 'Rear step bumper');
  for (const z of [-1.045, 1.045]) {
    for (const [y, mat] of [
      [1.16, 'amber'],
      [0.98, 'red'],
      [0.8, 'light'],
    ]) {
      endLight(3.863, y, z, 0.073, 'dark');
      endLight(3.884, y, z, 0.061, 'trim');
      endLight(3.906, y, z, 0.05, mat);
    }
    endLight(3.895, 0.53, z, 0.035, 'red');
    box(0.025, 0.028, 0.16, 3.896, 3.1, z, 'red', 'Rear clearance marker');
  }
  box(0.027, 0.095, 0.25, 3.913, 0.56, 0.985, 'white', 'Rear plate');
  // Smaller tires and forward rear axle leave the long rear overhang seen in the references.
  box(7.45, 0.15, 1.32, -0.1, 0.42, 0, 'dark', 'Chassis');
  for (const x of [frontAxle, rearAxle]) {
    cylinder(0.06, 2.19, x, 0.47, 0, 'dark', 12);
    for (const sign of [-1, 1]) {
      const z = sign * 1.15;
      part(new THREE.TorusGeometry(0.34, 0.13, 10, 36), 'rubber', x, 0.47, z, 'Tire');
      cylinder(0.33, 0.245, x, 0.47, z, 'rubber', 36);
      cylinder(0.255, 0.273, x, 0.47, z, 'trim', 32);
      cylinder(0.178, 0.282, x, 0.47, z, 'dark', 32);
      cylinder(0.11, 0.318, x, 0.47, z, 'trim', 24);
      cylinder(0.061, 0.343, x, 0.47, z, 'dark', 16);
      for (let j = 0; j < 8; j++) {
        const a = (j * Math.PI) / 4;
        cylinder(
          0.033,
          0.012,
          x + Math.sin(a) * 0.202,
          0.47 + Math.cos(a) * 0.202,
          z + sign * 0.144,
          'dark',
          10,
        );
        cylinder(
          0.012,
          0.02,
          x + Math.sin(a) * 0.087,
          0.47 + Math.cos(a) * 0.087,
          z + sign * 0.169,
          'light',
          6,
        );
      }
      for (const depth of [-0.078, 0.078])
        part(new THREE.TorusGeometry(0.444, 0.01, 4, 36), 'dark', x, 0.47, z + depth);
      box(0.065, 0.28, 0.45, x + 0.52, 0.3, z, 'dark', 'Mud flap');
    }
  }
  const brandMaterial = primeDecal();
  for (const sign of [-1, 1]) {
    const brand = part(
      new THREE.PlaneGeometry(4.42, 1.73),
      brandMaterial,
      1.12,
      2.32,
      sign * 1.235,
      'Prime side graphic',
    );
    if (sign < 0) brand.rotation.y = Math.PI;
  }
  const rearBrand = part(
    new THREE.PlaneGeometry(0.9, 0.45),
    amazonDecal('Original_amazon_rear_decal'),
    3.916,
    2.27,
    0,
    'Small rear Amazon graphic',
  );
  rearBrand.rotation.y = Math.PI / 2;
  return finish(van, 'Stepvan_', {
    author: 'Dispatch',
    source:
      'Original procedural mesh, modeled from four front, rear, passenger and driver photo references supplied by the user',
    units: 'metres',
    style: 'Simplified Freightliner / Utilimaster-inspired step van',
  });
}
