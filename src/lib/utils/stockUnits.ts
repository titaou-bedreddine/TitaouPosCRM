import type { PackagingDef } from '../types';

/**
 * Decomposes a base-unit stock count into the product's packaging units,
 * largest first with remainders: 1404 bottles of a 672-palette / 6-fardeau
 * product → "2 Palette, 1 Fardeau, 0 u" — plus the raw base count.
 */
export function decomposeStock(
  stock: number,
  packagings: Array<{ name: string; units_per_package: number }>): string {
  const sorted = [...packagings].sort((a, b) => b.units_per_package - a.units_per_package);
  let rest = Math.round(stock);
  const parts: string[] = [];
  for (const pk of sorted) {
    const upp = Math.max(1, Math.round(pk.units_per_package));
    const count = Math.floor(rest / upp);
    rest -= count * upp;
    if (count > 0) parts.push(`${count} ${pk.name}`);
  }
  if (rest > 0 || parts.length === 0) parts.push(`${rest} u`);
  return parts.join(' · ');
}
