import http from 'node:http';
import type { AddressInfo } from 'node:net';

// Paycom's API for one made-up client, as a connection with an SID and a Token meets it: it
// answers only requests signed with them, refuses a function the client has switched off, and
// pages its lists, saying how many rows there are in all.

/** One punch: when, in seconds since the epoch, and Paycom's two-letter kind. */
export type ApiPunch = { at: number; type: string };
export type ApiEmployee = {
  code: string;
  name: string;
  department: string;
  station: string;
  position: string;
  punches: ApiPunch[];
};

export interface PaycomApi {
  /** Where it answers, as `DISPATCH_FIXTURE_PROVIDER_URL` names it. */
  url: string;
  /** The functions the client has switched off, by the last word of their address:
   * `employeedirectory`, `employee`, `punchhistory` or `category`. */
  off: Set<string>;
  /** The path and query of every request it answered with rows. */
  answered: string[];
  close(): Promise<void>;
}

/** The time a clock in `zone` shows `clock` (HH:MM) on `date` (YYYY-MM-DD), in seconds since
 * the epoch. */
export function clockTime(date: string, clock: string, zone: string): number {
  const wall = Date.parse(`${date}T${clock}:00Z`);
  const shown = new Intl.DateTimeFormat('en-CA', {
    timeZone: zone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
    .formatToParts(wall)
    .reduce<Record<string, string>>((parts, part) => ({ ...parts, [part.type]: part.value }), {});
  const there = Date.parse(
    `${shown.year}-${shown.month}-${shown.day}T${shown.hour}:${shown.minute}:00Z`,
  );
  return (wall - (there - wall)) / 1000;
}

const ROOT = '/v4/rest/index.php/api/';
/** The most rows it puts on a page, however many are asked for, so that a short roster pages. */
const PAGE = 2;

export async function paycomApi(client: {
  sid: string;
  token: string;
  employees: ApiEmployee[];
}): Promise<PaycomApi> {
  const off = new Set<string>();
  const answered: string[] = [];
  const signed = `Basic ${Buffer.from(`${client.sid}:${client.token}`).toString('base64')}`;
  const server = http.createServer((request, response) => {
    const url = new URL(request.url ?? '/', 'http://fixture.dispatch.invalid');
    const answer = (status: number, body: unknown, headers: Record<string, string> = {}) => {
      response.writeHead(status, { 'content-type': 'application/json', ...headers });
      response.end(JSON.stringify(body));
    };
    if (!url.pathname.startsWith(ROOT)) return answer(404, { result: false });
    if (request.headers.authorization !== signed) return answer(401, { result: false });
    const path = url.pathname.slice(ROOT.length).replace(/\/$/, '');
    const name = path.split('/').at(-1)!;
    const fn = path.startsWith('v1/employee/') && name !== 'punchhistory' ? 'employee' : name;
    if (off.has(fn)) return answer(403, { result: false });
    /** A list, a page of it at a time. */
    const list = (rows: unknown[]) => {
      const page = Number(url.searchParams.get('page') ?? 1);
      const size = Math.min(Number(url.searchParams.get('pagesize') ?? 25), PAGE);
      const from = (page - 1) * size;
      answered.push(`${path}${url.search}`);
      answer(
        from + size < rows.length ? 206 : 200,
        { result: true, data: rows.slice(from, from + size) },
        { 'x-total-count': String(rows.length) },
      );
    };
    if (path === 'v1/employeedirectory') {
      return list(
        client.employees.map((employee) => ({
          eecode: employee.code,
          eename: employee.name,
          eestatus: 'A',
          deptdesc: employee.department,
          cat1: 'DRV',
          cat2: employee.station,
        })),
      );
    }
    if (path === 'v1/cl/category') {
      return list([
        { catcode: 1, catdesc: 'Job' },
        { catcode: 2, catdesc: 'Delivery Station Code' },
      ]);
    }
    const [, code, rest] = /^v1\/employee\/([A-Za-z0-9]+)(?:\/(punchhistory))?$/.exec(path) ?? [];
    const employee = client.employees.find((each) => each.code === code);
    if (!employee) return answer(404, { result: false });
    if (!rest) {
      answered.push(path);
      return answer(200, { result: true, data: { position: employee.position } });
    }
    const from = Number(url.searchParams.get('startdate'));
    const to = Number(url.searchParams.get('enddate'));
    list(
      employee.punches
        .filter((punch) => punch.at >= from && punch.at <= to)
        .map((punch) => ({
          entrytype: '1',
          punchtime: String(punch.at),
          punchtype: punch.type,
          eecode: employee.code,
          isdeleted: 0,
        })),
    );
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    url: `http://fixture.dispatch.invalid:${(server.address() as AddressInfo).port}`,
    off,
    answered,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  };
}
