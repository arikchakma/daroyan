import type { ChartSeries } from '~/components/servers/metric-chart';

type MetricGroupListProps = {
  series: ChartSeries[];
  formatValue: (value: number) => string;
};

export function MetricGroupList(props: MetricGroupListProps) {
  const { series, formatValue } = props;

  return (
    <ul aria-label="Groups" className="flex flex-col divide-y divide-border-ring">
      {series.map((line) => {
        const last = line.values.filter((value) => value !== null).at(-1);

        return (
          <li key={line.label} className="flex items-center gap-2.5 py-1.75 text-caption">
            <span className="size-2 shrink-0 rounded-full" style={{ backgroundColor: line.color }} />
            <span className="min-w-0 flex-1 truncate font-mono text-code-small text-text-secondary">{line.label}</span>
            <span className="font-medium text-text-primary tabular-nums">
              {last === undefined ? '—' : formatValue(last)}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
