/**
 * LOCAL "today" as YYYY-MM-DD. `new Date().toISOString()` is UTC — on
 * UTC+ machines after ~23:00 local it yields TOMORROW's date, silently
 * shifting every default date filter. All "today" defaults must use this.
 */
export function localTodayISO(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
