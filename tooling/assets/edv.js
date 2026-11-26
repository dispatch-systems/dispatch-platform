import * as THREE from 'three';
import { amazonDecal, createKit, finish, primeDecal, roundedOutline } from './kit.js';

// Original stylized mesh of the electric delivery van, authored for Dispatch from the
// owner's photographs. Same axes as the step van: X = length (front is negative X), Y = up,
// Z = width, +Z the driver side. Units approximately metres.
export function createEdv() {
  const paints = {
    body: new THREE.MeshStandardMaterial({ color: '#3f5a66', roughness: 0.55, metalness: 0.15 }),
    skirt: new THREE.MeshStandardMaterial({ color: '#283038', roughness: 0.9 }),
    trim: new THREE.MeshStandardMaterial({ color: '#8d9aa1', roughness: 0.45, metalness: 0.5 }),
    dark: new THREE.MeshStandardMaterial({ color: '#151b20', roughness: 0.85 }),
    rubber: new THREE.MeshStandardMaterial({ color: '#16191e', roughness: 0.95 }),
    glass: new THREE.MeshStandardMaterial({ color: '#1f3038', roughness: 0.25, metalness: 0.2 }),
    reflection: new THREE.MeshStandardMaterial({
      color: '#5f7a82',
      roughness: 0.6,
      metalness: 0.1,
    }),
    lens: new THREE.MeshStandardMaterial({ color: '#9aa6ad', roughness: 0.3, metalness: 0.4 }),
    light: new THREE.MeshStandardMaterial({
      color: '#eef5ed',
      roughness: 0.35,
      emissive: '#dceaf0',
      emissiveIntensity: 0.35,
    }),
    amber: new THREE.MeshStandardMaterial({
      color: '#ffa23b',
      roughness: 0.5,
      emissive: '#ff8217',
      emissiveIntensity: 0.2,
    }),
    red: new THREE.MeshStandardMaterial({
      color: '#e0454a',
      roughness: 0.5,
      emissive: '#d1202c',
      emissiveIntensity: 0.3,
    }),
    door: new THREE.MeshStandardMaterial({ color: '#4cb7e2', roughness: 0.6 }),
    wheel: new THREE.MeshStandardMaterial({ color: '#3a4148', roughness: 0.6, metalness: 0.3 }),
    white: new THREE.MeshStandardMaterial({ color: '#e8edf3', roughness: 0.7 }),
  };
  const { van, part, box, cylinder, tube, polygon, skin, outline, mappedSkin, mappedOutline } =
    createKit('Dispatch_EDV', paints);
  const width = 2.4,
    frontAxle = -2.35,
    rearAxle = 2.2,
    wheelRadius = 0.44,
    arch = 0.72;
  const front = -3.4,
    rear = 3.35,
    side = width / 2;
  const v2 = (p) => new THREE.Vector2(p[0], p[1]);
  // Rounds the path's corner at `at`, between the line from `from` and the line to `to`.
  function corner(shape, from, at, to, r) {
    const a = v2(from).sub(v2(at)).setLength(r).add(v2(at));
    const b = v2(to).sub(v2(at)).setLength(r).add(v2(at));
    shape.lineTo(a.x, a.y);
    shape.quadraticCurveTo(at[0], at[1], b.x, b.y);
  }
  // A side outline extruded across the van with rounded edges; `bevel` rounds every edge
  // and grows the outline by the same amount, so outlines are drawn inset by it.
  function shell(shape, bevel, material, name, across = width) {
    const geometry = new THREE.ExtrudeGeometry(shape, {
      depth: across - 2 * bevel,
      bevelEnabled: true,
      bevelThickness: bevel,
      bevelSize: bevel,
      bevelSegments: 5,
      curveSegments: 16,
    });
    geometry.translate(0, 0, -(across / 2 - bevel));
    return part(geometry, material, 0, 0, 0, name);
  }
  function slab(x0, x1, y0, y1, material, name, bevel = 0.21) {
    const shape = roundedOutline(
      [
        [x0 + bevel, y0 + bevel],
        [x0 + bevel, y1 - bevel],
        [x1 - bevel, y1 - bevel],
        [x1 - bevel, y0 + bevel],
      ],
      0.03,
    );
    return shell(shape, bevel, material, name, width + 0.02);
  }

  // One rounded body: short nose, steep windshield into a deep forehead, flat roof and sides.
  // The outline is inset by `bevel`; the body's surface stands `bevel` outside it.
  const bevel = 0.2,
    sill = 0.55,
    roof = 2.85,
    top = roof + bevel;
  const windshieldBase = [-2.9, 1.4],
    brow = [-2.15, roof],
    browRadius = 0.42;
  const profile = [
    [front + bevel, sill, 0.12],
    [front + bevel, 1.2, 0.1],
    [...windshieldBase, 0.15],
    [...brow, browRadius],
    [rear - bevel, roof, 0.12],
    [rear - bevel, sill, 0.1],
  ];
  const body = new THREE.Shape();
  const start = [frontAxle - arch, sill],
    end = [rearAxle + arch, sill];
  body.moveTo(...start);
  profile.forEach(([x, y, r], i) =>
    corner(body, profile[i - 1] ?? start, [x, y], profile[i + 1] ?? end, r),
  );
  body.lineTo(...end);
  body.absarc(rearAxle, sill, arch, 0, Math.PI, false);
  body.lineTo(frontAxle + arch, sill);
  body.absarc(frontAxle, sill, arch, 0, Math.PI, false);
  body.closePath();
  shell(body, bevel, 'body', 'Rounded body');

  // Dark lower cladding: bumper, sill between the wheels, rear corner, and arch flares.
  slab(front - 0.03, frontAxle - 0.52, 0.3, 0.76, 'skirt', 'Front bumper');
  slab(frontAxle + 0.5, rearAxle - 0.5, 0.3, 0.74, 'skirt', 'Sill cladding');
  slab(rearAxle + 0.5, rear + 0.02, 0.3, 0.78, 'skirt', 'Rear corner cladding');
  for (const x of [frontAxle, rearAxle]) {
    for (const sign of [-1, 1]) {
      part(
        new THREE.TorusGeometry(arch - 0.18, 0.05, 6, 28, Math.PI),
        'skirt',
        x,
        sill,
        sign * (side - 0.03),
        'Arch flare',
      );
    }
    box(1.1, 0.5, width - 0.7, x, 0.95, 0, 'dark', 'Wheel well liner');
  }

  // Windshield: one large pane on the sloped face, wrapping into the side glass.
  const base = v2(windshieldBase),
    rise = v2(brow).sub(base).normalize(),
    normal = new THREE.Vector2(-rise.y, rise.x);
  const windshieldMap = (u, t, lift = 0.012) => {
    const p = base
      .clone()
      .addScaledVector(rise, t)
      .addScaledVector(normal, bevel + lift);
    return [p.x, p.y, u];
  };
  const pane = roundedOutline(
    [
      [-0.97, 0.14],
      [-0.97, 1.16],
      [0.97, 1.16],
      [0.97, 0.14],
    ],
    0.12,
  );
  mappedSkin(pane, 'glass', windshieldMap, 'Windshield');
  mappedOutline(pane, (u, t) => windshieldMap(u, t, 0.016), 0.016, 'dark');
  for (const u of [-0.55, 0.35]) {
    const streak = roundedOutline(
      [
        [u, 0.26],
        [u + 0.2, 1.06],
        [u + 0.24, 1.06],
        [u + 0.04, 0.26],
      ],
      0.01,
    );
    mappedSkin(streak, 'reflection', (a, t) => windshieldMap(a, t, 0.016), 'Windshield reflection');
  }
  tube([windshieldMap(-0.15, 0.18, 0.03), windshieldMap(-0.85, 0.16, 0.03)], 0.014, 'dark', 1);
  tube([windshieldMap(0.75, 0.18, 0.03), windshieldMap(0.05, 0.16, 0.03)], 0.014, 'dark', 1);

  // Forehead cameras and clearance lights, facing out from the middle of the brow's curve.
  const browCurve = new THREE.QuadraticBezierCurve(
    v2(brow).addScaledVector(rise, -browRadius),
    v2(brow),
    v2(brow).add(new THREE.Vector2(browRadius, 0)),
  );
  const browTangent = browCurve.getTangent(0.5),
    browPoint = browCurve
      .getPoint(0.5)
      .add(new THREE.Vector2(-browTangent.y, browTangent.x).multiplyScalar(bevel));
  for (const [z, mat] of [
    [-0.78, 'amber'],
    [-0.4, 'dark'],
    [-0.1, 'dark'],
    [0.1, 'dark'],
    [0.4, 'dark'],
    [0.78, 'amber'],
  ]) {
    const light = cylinder(0.035, 0.05, browPoint.x, browPoint.y, z, mat, 12);
    light.rotation.set(0, 0, Math.atan2(browTangent.y, browTangent.x));
  }

  // Front face: the black band with ring headlights and the wordmark, the grille below.
  const faceMap = (dx) => (u, v) => [front - dx, v, u];
  mappedSkin(
    roundedOutline(
      [
        [-0.95, 0.78],
        [-0.95, 1.08],
        [0.95, 1.08],
        [0.95, 0.78],
      ],
      0.14,
    ),
    'dark',
    faceMap(0.006),
    'Headlight band',
  );
  for (const z of [-0.72, 0.72]) {
    const ring = part(new THREE.TorusGeometry(0.135, 0.022, 8, 32), 'light', front - 0.02, 0.93, z);
    ring.rotation.y = Math.PI / 2;
    const lens = part(
      new THREE.CylinderGeometry(0.112, 0.112, 0.02, 28),
      'lens',
      front - 0.012,
      0.93,
      z,
    );
    lens.rotation.z = Math.PI / 2;
    const bulb = part(
      new THREE.CylinderGeometry(0.04, 0.04, 0.02, 16),
      'light',
      front - 0.02,
      0.93,
      z,
    );
    bulb.rotation.z = Math.PI / 2;
    box(0.02, 0.05, 0.1, front - 0.035, 0.62, z * 1.42, 'amber', 'Bumper corner light');
  }
  mappedSkin(
    roundedOutline(
      [
        [-0.68, 0.56],
        [-0.68, 0.66],
        [0.68, 0.66],
        [0.68, 0.56],
      ],
      0.04,
    ),
    'dark',
    faceMap(0.036),
    'Grille slot',
  );
  for (let i = -8; i <= 8; i++) box(0.01, 0.09, 0.012, front - 0.04, 0.61, i * 0.075, 'skirt');
  mappedSkin(
    roundedOutline(
      [
        [-0.17, 0.38],
        [-0.17, 0.5],
        [0.17, 0.5],
        [0.17, 0.38],
      ],
      0.02,
    ),
    'dark',
    faceMap(0.036),
    'Front plate recess',
  );

  // Both sides: the entry door and its window, the mirror, panel seams and markers.
  const doorOutline = new THREE.Shape();
  doorOutline.moveTo(-2.856, 1.4);
  doorOutline.lineTo(-2.07, 2.7);
  doorOutline.lineTo(-1.3, 2.7);
  doorOutline.lineTo(-1.3, 0.8);
  doorOutline.absarc(frontAxle, sill, arch + 0.04, 0.335, 2.3, false);
  doorOutline.closePath();
  const doorGlass = roundedOutline(
    [
      [-2.7, 1.45],
      [-2.05, 2.56],
      [-1.42, 2.56],
      [-1.42, 1.45],
    ],
    [0.06, 0.12, 0.1, 0.06],
  );
  for (const sign of [-1, 1]) {
    const z = sign * side;
    outline(doorOutline, z + sign * 0.008, 0.012);
    skin(doorGlass, z + sign * 0.006, 'glass', 'Door window');
    outline(doorGlass, z + sign * 0.012, 0.02);
    polygon(
      [
        [-2.42, 1.62, z + sign * 0.01],
        [-2.36, 1.62, z + sign * 0.01],
        [-1.98, 2.36, z + sign * 0.01],
        [-2.04, 2.36, z + sign * 0.01],
      ],
      'reflection',
    );
    box(0.05, 0.14, 0.03, -1.4, 1.32, z + sign * 0.012, 'dark', 'Door handle');
    // A tall black mirror on a short arm at the front of the door.
    box(0.08, 0.06, 0.2, -2.6, 1.62, sign * (side + 0.08), 'dark', 'Mirror arm');
    box(0.14, 0.4, 0.2, -2.6, 1.82, sign * (side + 0.2), 'dark', 'Mirror');
    box(0.012, 0.32, 0.15, -2.53, 1.82, sign * (side + 0.2), 'trim', 'Mirror glass');
    for (const [x, y0] of [
      [0.0, 0.8],
      [1.25, 0.8],
      [2.45, 1.32],
    ])
      box(0.02, 2.72 - y0, 0.012, x, (2.72 + y0) / 2, z + sign * 0.006, 'dark', 'Panel seam');
    box(0.14, 0.04, 0.03, -1.6, 0.6, sign * (side + 0.01), 'amber', 'Side marker');
    box(0.12, 0.04, 0.03, rear - 0.25, 0.6, sign * (side + 0.01), 'red', 'Rear side marker');
  }

  // The rear: a dark horseshoe frame with red light bars around the light blue roll-up door.
  const frameShape = roundedOutline(
    [
      [-side - 0.01, 0.32],
      [-side - 0.01, top - 0.01],
      [side + 0.01, top - 0.01],
      [side + 0.01, 0.32],
    ],
    [0.05, 0.22, 0.22, 0.05],
  );
  frameShape.holes.push(
    roundedOutline(
      [
        [-0.86, 0.5],
        [-0.86, top - 0.23],
        [0.86, top - 0.23],
        [0.86, 0.5],
      ],
      [0.02, 0.1, 0.1, 0.02],
    ),
  );
  const frame = part(
    new THREE.ExtrudeGeometry(frameShape, {
      depth: 0.18,
      bevelEnabled: true,
      bevelThickness: 0.03,
      bevelSize: 0.03,
      bevelSegments: 3,
      curveSegments: 12,
    }),
    'dark',
    rear - 0.14,
    0,
    0,
    'Rear frame',
  );
  frame.rotation.y = Math.PI / 2;
  const doorMap = (dx) => (u, v) => [rear + dx, v, u];
  mappedSkin(
    roundedOutline(
      [
        [-0.86, 0.5],
        [-0.86, top - 0.23],
        [0.86, top - 0.23],
        [0.86, 0.5],
      ],
      [0.02, 0.1, 0.1, 0.02],
    ),
    'door',
    doorMap(0.005),
    'Roll-up door',
  );
  for (const y of [0.97, 1.44, 1.91, 2.38])
    box(0.008, 0.012, 1.68, rear + 0.01, y, 0, 'dark', 'Roll-up panel seam');
  box(0.02, 0.04, 0.2, rear + 0.015, 0.82, 0, 'dark', 'Roll-up handle');
  const back = rear + 0.085;
  for (const sign of [-1, 1]) {
    box(0.02, 1.55, 0.08, back, top - 0.98, sign * 1.06, 'red', 'Rear light bar');
    tube(
      [
        [back, top - 0.2, sign * 1.06],
        [back, top - 0.07, sign * 1.02],
        [back, top - 0.02, sign * 0.88],
      ],
      0.035,
      'red',
      8,
    );
    box(0.08, 1.55, 0.02, rear - 0.04, top - 0.98, sign * (side + 0.052), 'red', 'Rear wrap light');
    box(0.02, 0.18, 0.07, back, 0.6, sign * 1.06, 'red', 'Rear reflector');
  }
  box(0.06, 0.06, 0.12, back + 0.01, top - 0.13, 0, 'dark', 'Rear camera');
  box(0.3, 0.1, 1.9, rear + 0.08, 0.36, 0, 'dark', 'Rear step');
  box(0.02, 0.1, 0.26, back, 0.41, 0.55, 'white', 'Rear plate');

  // Black steel wheels on tall tires.
  for (const x of [frontAxle, rearAxle]) {
    cylinder(0.06, width - 0.4, x, wheelRadius, 0, 'dark', 12);
    for (const sign of [-1, 1]) {
      const z = sign * 1.0;
      part(new THREE.TorusGeometry(0.31, 0.13, 10, 36), 'rubber', x, wheelRadius, z, 'Tire');
      cylinder(0.32, 0.24, x, wheelRadius, z, 'rubber', 36);
      cylinder(0.25, 0.25, x, wheelRadius, z, 'wheel', 32);
      cylinder(0.09, 0.27, x, wheelRadius, z, 'trim', 20);
      cylinder(0.05, 0.29, x, wheelRadius, z, 'dark', 12);
      for (let j = 0; j < 6; j++) {
        const a = (j * Math.PI) / 3;
        cylinder(
          0.014,
          0.28,
          x + Math.sin(a) * 0.15,
          wheelRadius + Math.cos(a) * 0.15,
          z,
          'trim',
          6,
        );
      }
    }
  }

  // Branding is authored here; no third-party texture or mesh is embedded.
  const prime = primeDecal();
  for (const sign of [-1, 1]) {
    const brand = part(
      new THREE.PlaneGeometry(4.0, 1.56),
      prime,
      0.95,
      2.05,
      sign * (side + 0.012),
      'Prime side graphic',
    );
    if (sign < 0) brand.rotation.y = Math.PI;
  }
  const wordmark = amazonDecal('Original_amazon_decal', '#f5f7fa');
  const frontBrand = part(
    new THREE.PlaneGeometry(0.5, 0.25),
    wordmark,
    front - 0.012,
    0.93,
    0,
    'Front Amazon graphic',
  );
  frontBrand.rotation.y = -Math.PI / 2;
  const rearBrand = part(
    new THREE.PlaneGeometry(0.8, 0.4),
    wordmark,
    rear + 0.012,
    2.47,
    0,
    'Rear Amazon graphic',
  );
  rearBrand.rotation.y = Math.PI / 2;
  return finish(van, 'EDV_', {
    author: 'Dispatch',
    source:
      'Original procedural mesh, modeled from front, rear and side photo references supplied by the owner',
    units: 'metres',
    style: 'Simplified Rivian electric delivery van',
  });
}
