import type { Schemas } from '@daroyan/client';

import { ChartLegend, getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import { DAY_MS } from '~/utils/format';
import { formatMs, formatUptime } from '~/utils/uptime';

const BUCKET_MS = 10 * 60 * 1000;

type ResponseChartsProps = {
  points: Schemas['UptimePoint'][];
  to: number;
};

export function ResponseCharts(props: ResponseChartsProps) {
  const { points, to } = props;

  const from = to - DAY_MS;
  const times = points.map((point) => point.at);
  const response = points.map((point) => point.response_ms ?? 0);
  const availability = points.map((point) => (point.total > 0 ? ((point.total - point.down) / point.total) * 100 : 100));
  const total = points.reduce((sum, point) => sum + point.total, 0);
  const down = points.reduce((sum, point) => sum + point.down, 0);
  const measured = points.flatMap((point) => (point.response_ms === null || point.response_ms === undefined ? [] : [point.response_ms]));
  const average = measured.length > 0 ? measured.reduce((sum, value) => sum + value, 0) / measured.length : null;
  const max = getNiceMax(response);
  const dns = points.map((point) => point.dns_ms ?? 0);
  const tls = points.map((point) => point.tls_ms ?? 0);
  const averageOf = (values: (number | null | undefined)[]) => {
    const present = values.filter((value): value is number => value !== null && value !== undefined);
    return present.length > 0 ? present.reduce((sum, value) => sum + value, 0) / present.length : null;
  };
  const dnsAverage = averageOf(points.map((point) => point.dns_ms));
  const tlsAverage = averageOf(points.map((point) => point.tls_ms));
  const handshakeMax = getNiceMax([...dns, ...tls]);

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
      <MetricChart
        title="Response"
        value={formatMs(average)}
        detail="average"
        series={[{ label: 'Response', values: response }]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={max}
        maxLabel={formatMs(max)}
        formatValue={formatMs}
      />
      <MetricChart
        title="DNS and TLS"
        value={formatMs(dnsAverage === null && tlsAverage === null ? null : (dnsAverage ?? 0) + (tlsAverage ?? 0))}
        detail="average"
        series={[
          { label: 'DNS', values: dns },
          { label: 'TLS', values: tls, isSecondary: true },
        ]}
        times={times}
        from={from}
        to={to}
        bucketMs={BUCKET_MS}
        max={handshakeMax}
        maxLabel={formatMs(handshakeMax)}
        formatValue={formatMs}
        legend={
          <ChartLegend
            items={[
              { label: `DNS ${formatMs(dnsAverage)}` },
              { label: `TLS ${formatMs(tlsAverage)}`, isSecondary: true },
            ]}
          />
        }
      />
      <MetricChart
        title="Availability"
        value={formatUptime(total > 0 ? (total - down) / total : null)}
        detail="last 24 hours"
        isValueCritical={down > 0}
        series={[{ label: 'Availability', values: availability }]}
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
