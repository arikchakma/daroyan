const BUCKETS = 24;

type SparklineProps = {
  values: number[];
};

export function Sparkline(props: SparklineProps) {
  const { values } = props;
  if (values.length < 2) {
    return null;
  }

  const size = Math.ceil(values.length / BUCKETS);
  const averages = Array.from({ length: Math.ceil(values.length / size) }, (_, index) => {
    const bucket = values.slice(index * size, (index + 1) * size);
    return bucket.reduce((sum, value) => sum + value, 0) / bucket.length;
  });

  const width = 60;
  const height = 32;
  const max = Math.max(...averages) || 1;
  const step = width / Math.max(averages.length - 1, 1);
  const coords = averages.map((value, index) => [index * step, height - (value / max) * height] as const);
  const line = coords.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(' ');
  const last = coords[coords.length - 1];

  return (
    <svg viewBox={`0 0 ${width} ${height}`} className="h-8 w-15 shrink-0 overflow-visible" aria-hidden="true">
      <polyline
        points={line}
        fill="none"
        strokeWidth="1.5"
        strokeLinejoin="round"
        strokeLinecap="round"
        className="stroke-chart-primary"
      />
      <circle cx={last[0]} cy={last[1]} r="1.8" className="fill-chart-primary" />
    </svg>
  );
}
