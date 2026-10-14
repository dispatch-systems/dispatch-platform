//! Correlate the workload's slow database turns with completed HTTP requests. Only
//! operational fields already logged by core are retained, never bodies or query strings.
export class DatabaseWork {
  private pending = '';
  private slow = new Map<string, { route: string; dspId: string | null }[]>();
  private requests = new Map<string, { route: string; dspId: string | null }>();
  private byRoute = new Map<
    string,
    { calls: number; queueMs: number; lockMs: number; workMs: number }
  >();

  feed(chunk: string) {
    const lines = (this.pending + chunk).split('\n');
    this.pending = lines.pop()!;
    for (const line of lines) {
      let event;
      try {
        event = JSON.parse(line);
      } catch {
        continue;
      }
      const fields = event.fields;
      if (!fields?.requestId) continue;
      if (event.event === 'http.request') {
        this.requests.set(fields.requestId, { route: fields.route, dspId: fields.dspId ?? null });
      } else if (event.event === 'database.slow') {
        const rows = this.slow.get(fields.requestId) ?? [];
        rows.push({ route: fields.route, dspId: fields.dspId ?? null });
        this.slow.set(fields.requestId, rows);
        const record = this.byRoute.get(fields.route) ?? {
          calls: 0,
          queueMs: 0,
          lockMs: 0,
          workMs: 0,
        };
        record.calls++;
        for (const key of ['queueMs', 'lockMs', 'workMs'] as const)
          record[key] = Math.max(record[key], fields[key]);
        this.byRoute.set(fields.route, record);
      }
    }
  }

  report() {
    let attributed = 0,
      mismatched = 0;
    for (const [id, rows] of this.slow) {
      const request = this.requests.get(id);
      for (const row of rows) {
        if (request && row.route === request.route && (!row.dspId || row.dspId === request.dspId))
          attributed++;
        else mismatched++;
      }
    }
    return {
      attributed,
      mismatched,
      routes: [...this.byRoute].map(([route, record]) => ({ route, ...record })),
    };
  }
}
