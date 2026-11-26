import * as THREE from 'three';
import { mergeGeometries, mergeVertices } from 'three/addons/utils/BufferGeometryUtils.js';

// Shared building blocks for the original sign-in vans. A van names its paints, builds its
// parts from these helpers, then calls `finish` to merge them for the shipped model.

/** A closed outline through `points`, each corner rounded by `radius` (or its own radius). */
export function roundedOutline(points, radius = 0.06) {
  const shape = new THREE.Shape();
  points.forEach((p, i) => {
    const prev = points[(i + points.length - 1) % points.length],
      next = points[(i + 1) % points.length];
    const incoming = new THREE.Vector2(prev[0] - p[0], prev[1] - p[1]);
    const outgoing = new THREE.Vector2(next[0] - p[0], next[1] - p[1]);
    const r = Array.isArray(radius) ? radius[i] : radius;
    const d = Math.min(r, incoming.length() * 0.3, outgoing.length() * 0.3);
    incoming.normalize().multiplyScalar(d);
    outgoing.normalize().multiplyScalar(d);
    if (i === 0) shape.moveTo(p[0] + incoming.x, p[1] + incoming.y);
    else shape.lineTo(p[0] + incoming.x, p[1] + incoming.y);
    shape.quadraticCurveTo(p[0], p[1], p[0] + outgoing.x, p[1] + outgoing.y);
  });
  shape.closePath();
  return shape;
}

/** A van group and the part helpers that add meshes painted from `paints` to it. */
export function createKit(name, paints) {
  const van = new THREE.Group();
  van.name = name;
  Object.entries(paints).forEach(([key, m]) => (m.name = key));
  function part(geometry, material, x = 0, y = 0, z = 0, name = '') {
    const mesh = new THREE.Mesh(geometry, paints[material] || material);
    mesh.position.set(x, y, z);
    mesh.name = name;
    van.add(mesh);
    return mesh;
  }
  function box(w, h, d, x, y, z, mat = 'body', name = '') {
    return part(new THREE.BoxGeometry(w, h, d), mat, x, y, z, name);
  }
  function cylinder(r, length, x, y, z, mat = 'trim', segments = 24) {
    const mesh = part(new THREE.CylinderGeometry(r, r, length, segments), mat, x, y, z);
    mesh.rotation.x = Math.PI / 2;
    return mesh;
  }
  function tube(points, r = 0.02, mat = 'trim', segments = 24) {
    const curve = new THREE.CatmullRomCurve3(points.map((p) => new THREE.Vector3(...p)));
    return part(new THREE.TubeGeometry(curve, segments, r, 6, false), mat);
  }
  function polygon(points, mat) {
    const vertices = [];
    for (let i = 1; i < points.length - 1; i++)
      vertices.push(...points[0], ...points[i], ...points[i + 1]);
    const geom = new THREE.BufferGeometry();
    geom.setAttribute('position', new THREE.Float32BufferAttribute(vertices, 3));
    geom.computeVertexNormals();
    const material = (paints[mat] || mat).clone();
    material.side = THREE.DoubleSide;
    return part(geom, material);
  }
  function skin(shape, z, material, name) {
    const mat = paints[material].clone();
    mat.side = THREE.DoubleSide;
    return part(new THREE.ShapeGeometry(shape, 8), mat, 0, 0, z, name);
  }
  function outline(shape, z, r = 0.023, material = 'dark') {
    const points = shape.getPoints(8),
      curve = new THREE.CurvePath();
    for (let i = 1; i < points.length; i++)
      curve.add(
        new THREE.LineCurve3(
          new THREE.Vector3(points[i - 1].x, points[i - 1].y, z),
          new THREE.Vector3(points[i].x, points[i].y, z),
        ),
      );
    return part(new THREE.TubeGeometry(curve, points.length * 2, r, 6, false), material);
  }
  function mappedSkin(shape, mat, map, name) {
    const geometry = new THREE.ShapeGeometry(shape, 10),
      positions = geometry.attributes.position;
    for (let i = 0; i < positions.count; i++) {
      const p = map(positions.getX(i), positions.getY(i));
      positions.setXYZ(i, ...p);
    }
    geometry.computeVertexNormals();
    const material = paints[mat].clone();
    material.side = THREE.DoubleSide;
    return part(geometry, material, 0, 0, 0, name);
  }
  function mappedOutline(shape, map, r = 0.015, mat = 'dark') {
    const points = shape.getPoints(10),
      curve = new THREE.CurvePath();
    for (let i = 1; i < points.length; i++)
      curve.add(
        new THREE.LineCurve3(
          new THREE.Vector3(...map(points[i - 1].x, points[i - 1].y)),
          new THREE.Vector3(...map(points[i].x, points[i].y)),
        ),
      );
    return part(new THREE.TubeGeometry(curve, points.length * 2, r, 6, false), mat);
  }
  return {
    van,
    part,
    box,
    cylinder,
    tube,
    polygon,
    skin,
    outline,
    mappedSkin,
    mappedOutline,
  };
}

function decalMaterial(canvas, name) {
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  const material = new THREE.MeshStandardMaterial({
    map: texture,
    transparent: true,
    depthWrite: false,
    roughness: 0.9,
    side: THREE.DoubleSide,
  });
  material.name = name;
  return material;
}

// Branding is authored here; no third-party texture or mesh is embedded.
/** The side graphic: "prime" over the smile, on a 2:1 transparent decal. */
export function primeDecal() {
  const label = document.createElement('canvas');
  label.width = 1024;
  label.height = 512;
  const ctx = label.getContext('2d');
  ctx.fillStyle = '#f5f7fa';
  ctx.font = '600 110px Inter, Arial, sans-serif';
  ctx.textAlign = 'right';
  ctx.fillText('prime', 1000, 205);
  ctx.fillStyle = '#00bddd';
  ctx.strokeStyle = '#00bddd';
  ctx.lineWidth = 22;
  ctx.lineCap = 'round';
  ctx.beginPath();
  ctx.moveTo(45, 229);
  ctx.bezierCurveTo(260, 331, 555, 350, 781, 250);
  ctx.quadraticCurveTo(795, 245, 792, 263);
  ctx.bezierCurveTo(553, 391, 257, 378, 42, 244);
  ctx.quadraticCurveTo(30, 233, 45, 229);
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(708, 233);
  ctx.quadraticCurveTo(756, 215, 796, 222);
  ctx.quadraticCurveTo(803, 249, 784, 288);
  ctx.stroke();
  return decalMaterial(label, 'Original_prime_decal');
}

/** The "amazon" wordmark over its smile, on a 2:1 transparent decal. */
export function amazonDecal(name, smile = '#21c2e5') {
  const label = document.createElement('canvas');
  label.width = 512;
  label.height = 256;
  const ctx = label.getContext('2d');
  ctx.fillStyle = '#f5f7fa';
  ctx.font = '700 80px Inter, Arial, sans-serif';
  ctx.textAlign = 'center';
  ctx.fillText('amazon', 256, 123);
  ctx.strokeStyle = smile;
  ctx.lineWidth = 8;
  ctx.lineCap = 'round';
  ctx.beginPath();
  ctx.moveTo(151, 146);
  ctx.quadraticCurveTo(235, 188, 305, 146);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(281, 143);
  ctx.lineTo(310, 140);
  ctx.lineTo(304, 163);
  ctx.stroke();
  return decalMaterial(label, name);
}

/** Merge opaque components by material for a small number of draw calls. */
export function finish(van, prefix, userData) {
  van.updateMatrixWorld(true);
  const buckets = new Map();
  const keep = [];
  for (const mesh of [...van.children]) {
    if (mesh.material.transparent) {
      keep.push(mesh);
      continue;
    }
    const key = mesh.material.name + ':' + mesh.material.side;
    if (!buckets.has(key)) buckets.set(key, { material: mesh.material, geometries: [] });
    let geometry = mesh.geometry.clone().applyMatrix4(mesh.matrixWorld);
    if (geometry.index) geometry = geometry.toNonIndexed();
    if (!geometry.attributes.uv)
      geometry.setAttribute(
        'uv',
        new THREE.Float32BufferAttribute(
          new Float32Array(geometry.attributes.position.count * 2),
          2,
        ),
      );
    buckets.get(key).geometries.push(geometry);
  }
  van.clear();
  for (const { material, geometries } of buckets.values()) {
    const merged = mergeGeometries(geometries, false);
    const indexed = mergeVertices(merged, 1e-5);
    merged.dispose();
    const mesh = new THREE.Mesh(indexed, material);
    mesh.name = prefix + material.name;
    van.add(mesh);
    geometries.forEach((g) => g.dispose());
  }
  keep.forEach((mesh) => van.add(mesh));
  van.userData = userData;
  return van;
}
