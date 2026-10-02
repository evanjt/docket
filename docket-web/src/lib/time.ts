/** Seconds since the epoch of an RFC 3339 stamp, or null. */
export function epoch(stamp: string | null | undefined): number | null {
  if (!stamp) return null;
  const ms = Date.parse(stamp);
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

/** A span of seconds the way the TUI words it: `45s`, `12m`, `3h 05m`, `4d 02h`. */
export function duration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${String(m % 60).padStart(2, '0')}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${String(h % 24).padStart(2, '0')}h`;
}

export function ago(stamp: string | null | undefined, now = Date.now() / 1000): string {
  const t = epoch(stamp);
  return t === null ? '' : `${duration(now - t)} ago`;
}

/** The local calendar day of a stamp, `2026-10-02`. */
export function day(t: number): string {
  const d = new Date(t * 1000);
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

/** A stamp in local time: the time alone for today, the date as well for anything older. */
export function stamp(s: string | null | undefined, now = Date.now() / 1000): string {
  const t = epoch(s);
  if (t === null) return '';
  const d = new Date(t * 1000);
  const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  return day(t) === day(now) ? hm : `${day(t)} ${hm}`;
}
