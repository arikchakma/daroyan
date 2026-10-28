import type { Schemas } from '@daroyan/client';

import { DAY_MS } from '~/utils/format';
import { getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import { formatRunTime } from '~/utils/crons';
import { formatUptime } from '~/utils/uptime';

const BUCKET_MS = 10 * 60 * 1000;

type RunChartsProps = {
  points: Schemas['CronPoint'][];
  to: number;
};

export function RunCharts(props: RunChartsProps) {
  const { points, to } = props;

  const from = to - DAY_MS;
  const times = points.map((point) => point.at);
  const durations = points.map((point) => point.duration_ms ?? 0);
  const delays = points.map((point) => Math.max(point.delay_ms ?? 0, 0));
  const success = points.map((point) => (point.total > 0 ? (point.ok / point.total) * 100 : 100));
  const averageOf = (values: (number | null | undefined)[]) => {
    const present = values.filter((value): value is number => value !== null && value !== undefined);
    return present.length > 0 ? present.reduce((sum, value) => sum + value, 0) / present.length : null;
  };
  const total = points.reduce((sum, point) => sum + point.total, 0);
  const ok = points.reduce((sum, point) => sum + point.ok, 0);
  const durationMax = getNiceMax(durations);
  const delayMax = getNiceMax(delays);
  const delayAverage = averageOf(points.map((point) => point.delay_ms));

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
      <MetricChart
        title="Duration"
        value={formatRunTime(averageOf(points.map((point) => point.duration_ms)))}
        detail="average"
        series={[{ label: 'Duration', values: durations }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={durationMax}
        maxLabel={formatRunTime(durationMax)}
        formatValue={formatRunTime}
      />
      <MetricChart
        title="Start Delay"
        value={formatRunTime(delayAverage === null ? null : Math.max(delayAverage, 0))}
        detail="after the schedule"
        series={[{ label: 'Delay', values: delays }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={delayMax}
        maxLabel={formatRunTime(delayMax)}
        formatValue={formatRunTime}
      />
      <MetricChart
        title="Success Rate"
        value={formatUptime(total > 0 ? ok / total : null)}
        detail="last 24 hours"
        isValueCritical={ok < total}
        series={[{ label: 'Success', values: success }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={100}
        maxLabel="100%"
        formatValue={(value) => `${value.toFixed(1)}%`}
      />
    </div>
  );
}
