import type { CenterClass, TurnPoint } from "./types";

/** Five-degree density cells retain all exact observations for paginated picking. */
export const TURN_BINS = 72;
export function turnCell(x: number, y: number): number {
  const bin = (value: number) => Math.max(0, Math.min(TURN_BINS - 1, Math.floor((value + 180) / 5)));
  return bin(y) * TURN_BINS + bin(x);
}
export function turnMatches(point: TurnPoint, category: CenterClass | "all", size: string): boolean {
  const group =
    size === "all" ||
    (size === "9–15"
      ? point.cycle_size <= 15
      : size === "16–31"
        ? point.cycle_size >= 16 && point.cycle_size <= 31
        : point.cycle_size >= 32);
  return (
    group &&
    (category === "all" ||
      (category === "path" ? point.classes.every((c) => c === category) : point.classes.includes(category)))
  );
}
