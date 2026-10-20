// Writes backend/initials/glyphs.rs: Inter's capital letters at weight 600, as the dashboard
// draws people's initials, from the font the dashboard serves. Run it from the repository's root
// after the font changes: `node features/communications/tooling/initials-glyphs.mjs`.
//
// The font is a variable WOFF2: its tables are Brotli-compressed together, its outlines in
// WOFF2's own transformed form, and its weights a variation (gvar) of the regular one, all of
// which this reads. Only what A to Z need is read.
import fs from 'node:fs';
import zlib from 'node:zlib';

const FONT = 'app/frontend/public/assets/inter.woff2';
const OUT = 'features/communications/backend/initials/glyphs.rs';
const WEIGHT = 600;

/** The font's tables, by tag, uncompressed and as WOFF2 stores them. */
function tables(buf) {
  const known = [
    'cmap',
    'head',
    'hhea',
    'hmtx',
    'maxp',
    'name',
    'OS/2',
    'post',
    'cvt ',
    'fpgm',
    'glyf',
    'loca',
    'prep',
    'CFF ',
    'VORG',
    'EBDT',
    'EBLC',
    'gasp',
    'hdmx',
    'kern',
    'LTSH',
    'PCLT',
    'VDMX',
    'vhea',
    'vmtx',
    'BASE',
    'GDEF',
    'GPOS',
    'GSUB',
    'EBSC',
    'JSTF',
    'MATH',
    'CBDT',
    'CBLC',
    'COLR',
    'CPAL',
    'SVG ',
    'sbix',
    'acnt',
    'avar',
    'bdat',
    'bloc',
    'bsln',
    'cvar',
    'fdsc',
    'feat',
    'fmtx',
    'fvar',
    'gvar',
    'hsty',
    'just',
    'lcar',
    'mort',
    'morx',
    'opbd',
    'prop',
    'trak',
    'Zapf',
    'Silf',
    'Glat',
    'Gloc',
    'Feat',
    'Sill',
  ];
  if (buf.toString('latin1', 0, 4) !== 'wOF2') throw new Error(`${FONT} isn't WOFF2`);
  let off = 48;
  const base128 = () => {
    let value = 0;
    for (let i = 0; i < 5; i++) {
      const byte = buf[off++];
      value = value * 128 + (byte & 0x7f);
      if (!(byte & 0x80)) return value;
    }
    throw new Error('bad UIntBase128');
  };
  const listed = [];
  for (let i = buf.readUInt16BE(12); i > 0; i--) {
    const flags = buf[off++];
    let tag = known[flags & 0x3f];
    if ((flags & 0x3f) === 63) {
      tag = buf.toString('latin1', off, off + 4);
      off += 4;
    }
    const version = flags >> 6;
    const original = base128();
    const transformed = tag === 'glyf' || tag === 'loca' ? version === 0 : version !== 0;
    listed.push({ tag, length: transformed ? base128() : original, transformed });
  }
  const data = zlib.brotliDecompressSync(buf.subarray(off, off + buf.readUInt32BE(20)));
  const found = {};
  let at = 0;
  for (const table of listed) {
    found[table.tag] = { ...table, data: data.subarray(at, at + table.length) };
    at += table.length;
  }
  return found;
}

/** The glyph each of A to Z is drawn with, from the font's Unicode cmap. */
function letters(cmap) {
  const count = cmap.readUInt16BE(2);
  for (let i = 0; i < count; i++) {
    const platform = cmap.readUInt16BE(4 + i * 8);
    const encoding = cmap.readUInt16BE(6 + i * 8);
    const sub = cmap.subarray(cmap.readUInt32BE(8 + i * 8));
    if (!((platform === 3 && encoding === 1) || platform === 0) || sub.readUInt16BE(0) !== 4) {
      continue;
    }
    const segments = sub.readUInt16BE(6) / 2;
    const glyph = (code) => {
      for (let s = 0; s < segments; s++) {
        const end = sub.readUInt16BE(14 + s * 2);
        if (code > end) continue;
        const start = sub.readUInt16BE(16 + segments * 2 + s * 2);
        if (code < start) return 0;
        const delta = sub.readInt16BE(16 + segments * 4 + s * 2);
        const rangeAt = 16 + segments * 6 + s * 2;
        const range = sub.readUInt16BE(rangeAt);
        if (range === 0) return (code + delta) & 0xffff;
        const id = sub.readUInt16BE(rangeAt + range + (code - start) * 2);
        return id === 0 ? 0 : (id + delta) & 0xffff;
      }
      return 0;
    };
    return [...'ABCDEFGHIJKLMNOPQRSTUVWXYZ'].map((letter) => glyph(letter.charCodeAt(0)));
  }
  throw new Error('no Unicode cmap of format 4');
}

/** Each simple glyph's contours, as WOFF2's transformed glyf table holds them, up to `last`. */
function outlines(glyf, last) {
  const streams = [];
  let at = 36;
  for (let i = 0; i < 7; i++) {
    const size = glyf.readUInt32BE(8 + i * 4);
    streams.push({ data: glyf.subarray(at, at + size), at: 0 });
    at += size;
  }
  const [contours, points, flags, glyphs, composites, , instructions] = streams;
  const u8 = (s) => s.data[s.at++];
  const u16 = (s) => ((s.at += 2), s.data.readUInt16BE(s.at - 2));
  const i16 = (s) => ((s.at += 2), s.data.readInt16BE(s.at - 2));
  const u255 = (s) => {
    const code = u8(s);
    if (code === 253) return u16(s);
    if (code === 255) return u8(s) + 253;
    if (code === 254) return u8(s) + 506;
    return code;
  };
  const signed = (flag, value) => (flag & 1 ? value : -value);
  const found = [];
  for (let id = 0; id <= last; id++) {
    const count = i16(contours);
    if (count === 0) {
      found.push(null);
      continue;
    }
    if (count < 0) {
      // A composite: walk past its components, and their instructions if it has them.
      let more = true;
      let instructed = false;
      while (more) {
        const flag = u16(composites);
        composites.at += 2 + (flag & 0x0001 ? 4 : 2);
        composites.at += flag & 0x0008 ? 2 : flag & 0x0040 ? 4 : flag & 0x0080 ? 8 : 0;
        instructed ||= Boolean(flag & 0x0100);
        more = Boolean(flag & 0x0020);
      }
      if (instructed) instructions.at += u255(glyphs);
      found.push(null);
      continue;
    }
    const ends = [];
    let total = 0;
    for (let c = 0; c < count; c++) ends.push((total += u255(points)));
    const read = [];
    let x = 0;
    let y = 0;
    for (let p = 0; p < total; p++) {
      const flag = u8(flags);
      const on = !(flag & 0x80);
      const f = flag & 0x7f;
      const b = (n) => glyphs.data[glyphs.at + n];
      let dx;
      let dy;
      let size;
      if (f < 10) {
        [dx, dy, size] = [0, signed(f, ((f & 14) << 7) + b(0)), 1];
      } else if (f < 20) {
        [dx, dy, size] = [signed(f, (((f - 10) & 14) << 7) + b(0)), 0, 1];
      } else if (f < 84) {
        const b0 = f - 20;
        dx = signed(f, 1 + (b0 & 0x30) + (b(0) >> 4));
        dy = signed(f >> 1, 1 + ((b0 & 0x0c) << 2) + (b(0) & 0x0f));
        size = 1;
      } else if (f < 120) {
        const b0 = f - 84;
        dx = signed(f, 1 + (Math.floor(b0 / 12) << 8) + b(0));
        dy = signed(f >> 1, 1 + (((b0 % 12) >> 2) << 8) + b(1));
        size = 2;
      } else if (f < 124) {
        dx = signed(f, (b(0) << 4) + (b(1) >> 4));
        dy = signed(f >> 1, ((b(1) & 0x0f) << 8) + b(2));
        size = 3;
      } else {
        dx = signed(f, (b(0) << 8) + b(1));
        dy = signed(f >> 1, (b(2) << 8) + b(3));
        size = 4;
      }
      glyphs.at += size;
      read.push({ x: (x += dx), y: (y += dy), on });
    }
    instructions.at += u255(glyphs);
    const shaped = [];
    let start = 0;
    for (const end of ends) {
      shaped.push(read.slice(start, end));
      start = end;
    }
    found.push(shaped);
  }
  return found;
}

/** The weight's place on the font's own scale of it, -1 to 1, after its avar map. */
function normalized(fvar, avar, weight) {
  const axes = fvar.readUInt16BE(4);
  const [min, normal, max] = [4, 8, 12].map((at) => fvar.readInt32BE(axes + at) / 65536);
  let v = weight < normal ? (weight - normal) / (normal - min) : (weight - normal) / (max - normal);
  const pairs = avar.readUInt16BE(8);
  const map = [];
  for (let i = 0; i < pairs; i++) {
    map.push([avar.readInt16BE(10 + i * 4) / 16384, avar.readInt16BE(12 + i * 4) / 16384]);
  }
  for (let i = 1; i < map.length; i++) {
    const [[a, a2], [b, b2]] = [map[i - 1], map[i]];
    if (v <= b) return a2 + ((v - a) * (b2 - a2)) / (b - a);
  }
  return v;
}

/** How far each of a glyph's points, and its four phantom points, move at `coord`. */
function variation(gvar, glyph, shapes, coord) {
  const axes = gvar.readUInt16BE(4);
  const shared = gvar.readUInt32BE(8);
  const long = gvar.readUInt16BE(14) & 1;
  const array = gvar.readUInt32BE(16);
  const offset = (i) => (long ? gvar.readUInt32BE(20 + i * 4) : gvar.readUInt16BE(20 + i * 2) * 2);
  const all = shapes.flat();
  const count = all.length + 4;
  const moved = Array.from({ length: count }, () => ({ x: 0, y: 0 }));
  if (offset(glyph) === offset(glyph + 1)) return moved;
  const data = gvar.subarray(array + offset(glyph), array + offset(glyph + 1));
  const tuples = data.readUInt16BE(0);
  let serialized = data.readUInt16BE(2);
  const read = (at) => ({ at });
  const numbers = (cursor) => {
    let n = data[cursor.at++];
    if (n === 0) return null;
    if (n & 0x80) n = ((n & 0x7f) << 8) | data[cursor.at++];
    const found = [];
    let last = 0;
    while (found.length < n) {
      const control = data[cursor.at++];
      const run = (control & 0x7f) + 1;
      for (let i = 0; i < run; i++) {
        const step = control & 0x80 ? data.readUInt16BE((cursor.at += 2) - 2) : data[cursor.at++];
        found.push((last += step));
      }
    }
    return found;
  };
  const deltas = (cursor, n) => {
    const found = [];
    while (found.length < n) {
      const control = data[cursor.at++];
      const run = (control & 0x3f) + 1;
      for (let i = 0; i < run; i++) {
        if (control & 0x80) found.push(0);
        else if (control & 0x40) found.push(data.readInt16BE((cursor.at += 2) - 2));
        else found.push(data.readInt8(cursor.at++));
      }
    }
    return found;
  };
  const cursor = read(serialized);
  const sharedPoints = tuples & 0x8000 ? numbers(cursor) : undefined;
  let header = 4;
  for (let t = 0; t < (tuples & 0x0fff); t++) {
    const size = data.readUInt16BE(header);
    const index = data.readUInt16BE(header + 2);
    header += 4;
    let peak;
    if (index & 0x8000) {
      peak = Array.from({ length: axes }, (_, a) => data.readInt16BE(header + a * 2) / 16384);
      header += axes * 2;
    } else {
      const at = shared + (index & 0x0fff) * axes * 2;
      peak = Array.from({ length: axes }, (_, a) => gvar.readInt16BE(at + a * 2) / 16384);
    }
    let start = peak.map((p) => Math.min(p, 0));
    let end = peak.map((p) => Math.max(p, 0));
    if (index & 0x4000) {
      start = Array.from({ length: axes }, (_, a) => data.readInt16BE(header + a * 2) / 16384);
      end = Array.from(
        { length: axes },
        (_, a) => data.readInt16BE(header + axes * 2 + a * 2) / 16384,
      );
      header += axes * 4;
    }
    const here = read(cursor.at);
    const points = index & 0x2000 ? numbers(here) : sharedPoints;
    const n = points ? points.length : count;
    const dx = deltas(here, n);
    const dy = deltas(here, n);
    cursor.at += size;
    let scalar = 1;
    for (let a = 0; a < axes; a++) {
      const [lo, p, hi, v] = [start[a], peak[a], end[a], coord[a]];
      if (p === 0 || v === p) continue;
      if (v <= lo || v >= hi) scalar = 0;
      else scalar *= v < p ? (v - lo) / (p - lo) : (hi - v) / (hi - p);
    }
    if (scalar === 0) continue;
    const touched = new Map();
    (points ?? [...Array(count).keys()]).forEach((point, i) =>
      touched.set(point, { x: dx[i], y: dy[i] }),
    );
    const delta = inferred(shapes, touched, count);
    delta.forEach((d, i) => {
      moved[i].x += d.x * scalar;
      moved[i].y += d.y * scalar;
    });
  }
  return moved;
}

/** Each point's delta: those `touched` names, and the rest of each contour's interpolated. */
function inferred(shapes, touched, count) {
  const deltas = Array.from({ length: count }, (_, i) => touched.get(i) ?? { x: 0, y: 0 });
  let first = 0;
  for (const contour of shapes) {
    const ids = contour.map((_, i) => first + i);
    const set = ids.filter((id) => touched.has(id));
    if (set.length === 1) {
      for (const id of ids) deltas[id] = touched.get(set[0]);
    } else if (set.length > 1) {
      for (const id of ids) {
        if (touched.has(id)) continue;
        const local = id - first;
        let before = set.filter((t) => t < id).at(-1) ?? set.at(-1);
        let after = set.find((t) => t > id) ?? set[0];
        const value = {};
        for (const axis of ['x', 'y']) {
          const [c, c1, c2] = [
            contour[local][axis],
            contour[before - first][axis],
            contour[after - first][axis],
          ];
          const [d1, d2] = [touched.get(before)[axis], touched.get(after)[axis]];
          if (c1 === c2) value[axis] = d1 === d2 ? d1 : 0;
          else if (c <= Math.min(c1, c2)) value[axis] = c1 < c2 ? d1 : d2;
          else if (c >= Math.max(c1, c2)) value[axis] = c1 > c2 ? d1 : d2;
          else value[axis] = d1 + ((c - c1) * (d2 - d1)) / (c2 - c1);
        }
        deltas[id] = value;
      }
    }
    first += contour.length;
  }
  return deltas;
}

const font = tables(fs.readFileSync(FONT));
if (!font.glyf.transformed) throw new Error('glyf is expected in WOFF2’s transformed form');
const ids = letters(font.cmap.data);
const shapes = outlines(font.glyf.data, Math.max(...ids));
const hmtx = font.hmtx.data;
const metrics = font.hhea.data.readUInt16BE(34);
const coord = [normalized(font.fvar.data, font.avar.data, WEIGHT)];
const units = font.head.data.readUInt16BE(18);
const cap = font['OS/2'].data.readInt16BE(88);
/** A contour as path data: from an on-curve point, lines and quadratic curves back to it. */
function path(contour) {
  const at = (p) => `${+p[0].toFixed(1)} ${+p[1].toFixed(1)}`;
  const mid = (a, b) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, true];
  let first = contour.findIndex((p) => p[2]);
  let points;
  if (first < 0) {
    points = [mid(contour[0], contour[1]), ...contour.slice(1), contour[0]];
  } else {
    points = [...contour.slice(first), ...contour.slice(0, first)];
  }
  points.push(points[0]);
  const drawn = [`M${at(points[0])}`];
  for (let i = 1; i < points.length;) {
    const p = points[i];
    if (p[2]) {
      drawn.push(`L${at(p)}`);
      i += 1;
    } else if (points[i + 1][2]) {
      drawn.push(`Q${at(p)} ${at(points[i + 1])}`);
      i += 2;
    } else {
      drawn.push(`Q${at(p)} ${at(mid(p, points[i + 1]))}`);
      i += 1;
    }
  }
  return `${drawn.join('')}Z`;
}

/** Path data in pieces of at most 96 characters, each broken before a command. */
function lines(path) {
  const pieces = [];
  let piece = '';
  for (const command of path.match(/[MLQZ][^MLQZ]*/g)) {
    if (piece && piece.length + command.length > 96) {
      pieces.push(piece);
      piece = '';
    }
    piece += command;
  }
  return [...pieces, piece];
}

const glyphs = ids.map((id, n) => {
  const contours = shapes[id];
  if (!contours) throw new Error(`${String.fromCharCode(65 + n)} isn't a simple glyph`);
  const advance = hmtx.readUInt16BE(4 * Math.min(id, metrics - 1));
  const moved = variation(font.gvar.data, id, contours, coord);
  let i = 0;
  const drawn = contours.map((contour) =>
    path(
      contour.map((point) => {
        const d = moved[i++];
        return [Math.round(point.x + d.x), Math.round(point.y + d.y), point.on];
      }),
    ),
  );
  const [left, right] = [moved[i], moved[i + 1]];
  return { advance: Math.round(advance + right.x - left.x), path: drawn.join('') };
});

const rust = [
  `//! Inter's capital letters at weight ${WEIGHT}, as the dashboard draws people's initials, written`,
  "//! by `tooling/initials-glyphs.mjs` from the font the dashboard serves. Don't edit it by hand:",
  '//! run that again.',
  '',
  '/// Font units to the em.',
  `pub const UNITS: f32 = ${units}.0;`,
  '/// The height of a capital letter, in font units.',
  `pub const CAP_HEIGHT: f32 = ${cap}.0;`,
  '',
  '/// A capital letter: how far it advances, and its outline as path data in font units, the',
  '/// baseline at 0 and up the way it grows: `M` to start a contour, `L` a line, `Q` a quadratic',
  '/// curve through its control point, `Z` back to its start.',
  'pub struct Glyph {',
  '    pub advance: f32,',
  "    pub path: &'static str,",
  '}',
  '',
  '/// A to Z.',
  'pub const LETTERS: [Glyph; 26] = [',
  ...glyphs.map(({ advance, path }, n) =>
    [
      `    // ${String.fromCharCode(65 + n)}`,
      '    Glyph {',
      `        advance: ${advance}.0,`,
      '        path: concat!(',
      ...lines(path).map((line) => `            "${line}",`),
      '        ),',
      '    },',
    ].join('\n'),
  ),
  '];',
  '',
].join('\n');
fs.writeFileSync(OUT, rust);
console.log(`Wrote ${OUT}: ${glyphs.length} letters at weight ${WEIGHT}.`);
