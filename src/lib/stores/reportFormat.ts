// Report print format: the user picks A4 or 70mm (thermal roll) before any
// report-style document is printed — Z-report (cash session), route sheets,
// truck-load sheets. The choice is per print (a small modal), so a one-off
// A4 print never disturbs the saved receipt paper width.
import { writable, get } from 'svelte/store';

export type ReportFormat = 'a4' | '70mm';

export const reportFormatPicker = writable<{
  title: string;
  resolve: (f: ReportFormat | null) => void;
} | null>(null);

/** Ask which format to print this report in. Resolves null on cancel. */
export function pickReportFormat(title: string): Promise<ReportFormat | null> {
  return new Promise((resolve) => {
    reportFormatPicker.set({ title, resolve });
  });
}

export function settleReportFormat(f: ReportFormat | null) {
  const p = get(reportFormatPicker);
  reportFormatPicker.set(null);
  p?.resolve(f);
}
