export function formatCost(value: number | null | undefined): string {
  return `$${Number(value || 0).toFixed(2)}`;
}
