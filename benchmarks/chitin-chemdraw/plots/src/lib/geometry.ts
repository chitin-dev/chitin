/** Display transforms and screen-space picking are independent of Solid reactivity. */
export const symlog = (value: number, threshold = 0.5) => Math.sign(value) * Math.log1p(Math.abs(value) / threshold);
export const inverseSymlog = (value: number, threshold = 0.5) =>
  Math.sign(value) * Math.expm1(Math.abs(value)) * threshold;
export interface ScreenPoint {
  index: number;
  x: number;
  y: number;
}
export class SpatialGrid {
  private cells = new Map<string, ScreenPoint[]>();
  constructor(readonly cellSize = 12) {}
  add(point: ScreenPoint): void {
    const key = `${Math.floor(point.x / this.cellSize)},${Math.floor(point.y / this.cellSize)}`;
    const cell = this.cells.get(key);
    if (cell) cell.push(point);
    else this.cells.set(key, [point]);
  }
  /** Return every overlapping candidate, not merely the last painted point. */
  nearest(x: number, y: number, radius = 8): number[] {
    const found: { index: number; distance: number }[] = [];
    for (let i = Math.floor((x - radius) / this.cellSize); i <= Math.floor((x + radius) / this.cellSize); i++) {
      for (let j = Math.floor((y - radius) / this.cellSize); j <= Math.floor((y + radius) / this.cellSize); j++) {
        for (const point of this.cells.get(`${i},${j}`) ?? []) {
          const distance = (point.x - x) ** 2 + (point.y - y) ** 2;
          if (distance <= radius ** 2) found.push({ index: point.index, distance });
        }
      }
    }
    return found.sort((a, b) => a.distance - b.distance || a.index - b.index).map((point) => point.index);
  }
  /** Hover needs only a nearest point and count, not a sort of thousands of ties. */
  inspect(x: number, y: number, radius = 8): { index: number | undefined; count: number } {
    let index: number | undefined;
    let best = Infinity;
    let count = 0;
    for (let i = Math.floor((x - radius) / this.cellSize); i <= Math.floor((x + radius) / this.cellSize); i++) {
      for (let j = Math.floor((y - radius) / this.cellSize); j <= Math.floor((y + radius) / this.cellSize); j++) {
        for (const point of this.cells.get(`${i},${j}`) ?? []) {
          const distance = (point.x - x) ** 2 + (point.y - y) ** 2;
          if (distance > radius ** 2) continue;
          count++;
          if (distance < best || (distance === best && point.index < (index ?? Infinity))) {
            best = distance;
            index = point.index;
          }
        }
      }
    }
    return { index, count };
  }
}
export interface View {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
}
/** Quantiles use linear interpolation only for display; benchmark definitions remain unchanged. */
export function quantile(sorted: number[], probability: number): number {
  if (!sorted.length) return NaN;
  const offset = (sorted.length - 1) * probability;
  const low = Math.floor(offset);
  return sorted[low] + (sorted[Math.ceil(offset)] - sorted[low]) * (offset - low);
}
