import type { ChartCurve } from '@tanstack/charts';

type ChartXY = readonly [number, number];

const TENSION = 0.18;

function clamp(value: number, a: number, b: number) {
  return Math.min(Math.max(value, Math.min(a, b)), Math.max(a, b));
}

function getSmoothPath(points: readonly ChartXY[]) {
  const [first] = points;
  if (!first) {
    return '';
  }

  let path = `M${first[0]},${first[1]}`;
  for (let index = 0; index < points.length - 1; index++) {
    const p0 = points[index - 1] ?? points[index]!;
    const p1 = points[index]!;
    const p2 = points[index + 1]!;
    const p3 = points[index + 2] ?? p2;

    const c1x = p1[0] + (p2[0] - p0[0]) * TENSION;
    const c1y = clamp(p1[1] + (p2[1] - p0[1]) * TENSION, p1[1], p2[1]);
    const c2x = p2[0] - (p3[0] - p1[0]) * TENSION;
    const c2y = clamp(p2[1] - (p3[1] - p1[1]) * TENSION, p1[1], p2[1]);

    path += `C${c1x},${c1y},${c2x},${c2y},${p2[0]},${p2[1]}`;
  }

  return path;
}

export const SMOOTH_CURVE: ChartCurve = {
  line: getSmoothPath,
  area: (top, bottom) => {
    const base = [...bottom].reverse().map((point) => `L${point[0]},${point[1]}`);
    return `${getSmoothPath(top)}${base.join('')}Z`;
  },
};
