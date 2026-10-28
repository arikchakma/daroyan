import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { ChartLegend, getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { serverMetricsOptions } from '~/queries/servers';
import type { MetricRange } from '~/queries/servers';
import { formatCount, formatPercent, formatSpeed, formatUsedOf, getPercent } from '~/utils/format';

const RANGE_MS: Record<MetricRange, number> = {
  '1h': 3_600_000,
  '6h': 6 * 3_600_000,
  '24h': 24 * 3_600_000,
  '7d': 7 * 24 * 3_600_000,
  '30d': 30 * 24 * 3_600_000,
};

const RANGES: { value: MetricRange; label: string }[] = [
  { value: '1h', label: 'Last Hour' },
  { value: '6h', label: 'Last 6 Hours' },
  { value: '24h', label: 'Last 24 Hours' },
  { value: '7d', label: 'Last 7 Days' },
  { value: '30d', label: 'Last 30 Days' },
];

type MetricsSectionProps = {
  serverId: string;
  host: Schemas['Host'];
  report?: Schemas['LastReport'] | null;
};

export function MetricsSection(props: MetricsSectionProps) {
  const { serverId, host, report } = props;

  const [range, setRange] = useState<MetricRange>('1h');
  const { data: metrics, dataUpdatedAt } = useQuery(serverMetricsOptions(serverId, range));
  const to = dataUpdatedAt || Date.now();
  const from = to - RANGE_MS[range];

  const points = metrics?.points ?? [];
  const times = points.map((point) => point.at);
  const pick = (read: (point: Schemas['MetricPoint']) => number) => points.map(read);
  const last = points.at(-1);
  const state = report?.state;

  const memPercent = pick((point) => getPercent(point.mem_used, host.mem_total));
  const swapPercent = pick((point) => getPercent(point.swap_used, host.swap_total));
  const diskRead = pick((point) => point.disk_read_speed);
  const diskWrite = pick((point) => point.disk_write_speed);
  const netIn = pick((point) => point.net_in_speed);
  const netOut = pick((point) => point.net_out_speed);
  const tcp = pick((point) => point.tcp_conn_count);
  const udp = pick((point) => point.udp_conn_count);
  const load = pick((point) => point.load1);
  const diskUsed = state?.disks.reduce((sum, disk) => sum + disk.disk_used, 0) ?? 0;
  const diskTotal = state?.disks.reduce((sum, disk) => sum + disk.disk_total, 0) ?? 0;
  const ioMax = getNiceMax([...diskRead, ...diskWrite]);
  const netMax = getNiceMax([...netIn, ...netOut]);
  const connMax = getNiceMax([...tcp, ...udp]);
  const loadMax = getNiceMax([...load, host.cpu_cores, state?.load1 ?? 0]);

  const charts: { key: string; title: string; value: string; detail?: string; isValueCritical?: boolean; series: ChartSeries[]; max: number; maxLabel: string; formatValue: (value: number) => string; limit?: number; legend?: { label: string; isSecondary?: boolean }[] }[] = [
    {
      key: 'cpu',
      title: 'CPU',
      value: formatPercent(state?.cpu ?? last?.cpu ?? 0),
      detail: `${host.cpu_cores} vCPU`,
      isValueCritical: (state?.cpu ?? 0) >= 90,
      series: [{ label: 'CPU', values: pick((point) => point.cpu) }],
      max: 100,
      maxLabel: '100%',
      formatValue: (value) => formatPercent(value, 1),
      limit: 90,
    },
    {
      key: 'memory',
      title: 'Memory',
      value: formatPercent(getPercent(state?.mem_used ?? 0, host.mem_total)),
      detail: formatUsedOf(state?.mem_used ?? 0, host.mem_total),
      series: [
        { label: 'Memory', values: memPercent },
        { label: 'Swap', values: swapPercent, isSecondary: true },
      ],
      max: 100,
      maxLabel: '100%',
      formatValue: (value) => formatPercent(value, 1),
      legend: [
        { label: `Memory ${formatPercent(getPercent(state?.mem_used ?? 0, host.mem_total))}` },
        { label: `Swap ${formatPercent(getPercent(state?.swap_used ?? 0, host.swap_total))}`, isSecondary: true },
      ],
    },
    {
      key: 'disk',
      title: 'Disk I/O',
      value: formatSpeed((diskRead.at(-1) ?? 0) + (diskWrite.at(-1) ?? 0)),
      detail: `${formatPercent(getPercent(diskUsed, diskTotal))} used`,
      series: [
        { label: 'Read', values: diskRead },
        { label: 'Write', values: diskWrite, isSecondary: true },
      ],
      max: ioMax,
      maxLabel: formatSpeed(ioMax),
      formatValue: formatSpeed,
      legend: [{ label: `Read ${formatSpeed(diskRead.at(-1) ?? 0)}` }, { label: `Write ${formatSpeed(diskWrite.at(-1) ?? 0)}`, isSecondary: true }],
    },
    {
      key: 'load',
      title: 'Load',
      value: (state?.load1 ?? 0).toFixed(2),
      detail: `${host.cpu_cores} cores`,
      isValueCritical: (state?.load1 ?? 0) > host.cpu_cores,
      series: [{ label: 'Load', values: load }],
      max: loadMax,
      maxLabel: loadMax.toString(),
      formatValue: (value) => value.toFixed(2),
      limit: host.cpu_cores,
    },
    {
      key: 'network',
      title: 'Network',
      value: formatSpeed((netIn.at(-1) ?? 0) + (netOut.at(-1) ?? 0)),
      detail: state?.nics.map((nic) => nic.name).slice(0, 2).join(', '),
      series: [
        { label: 'Down', values: netIn },
        { label: 'Up', values: netOut, isSecondary: true },
      ],
      max: netMax,
      maxLabel: formatSpeed(netMax),
      formatValue: formatSpeed,
      legend: [{ label: `Down ${formatSpeed(netIn.at(-1) ?? 0)}` }, { label: `Up ${formatSpeed(netOut.at(-1) ?? 0)}`, isSecondary: true }],
    },
    {
      key: 'connections',
      title: 'Connections',
      value: formatCount((state?.tcp_conn_count ?? 0) + (state?.udp_conn_count ?? 0)),
      detail: 'TCP and UDP',
      series: [
        { label: 'TCP', values: tcp },
        { label: 'UDP', values: udp, isSecondary: true },
      ],
      max: connMax,
      maxLabel: formatCount(connMax),
      formatValue: formatCount,
      legend: [{ label: `TCP ${formatCount(state?.tcp_conn_count ?? 0)}` }, { label: `UDP ${formatCount(state?.udp_conn_count ?? 0)}`, isSecondary: true }],
    },
  ];

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center">
        <h2 className="flex-1 text-title-section font-medium text-text-primary">Metrics</h2>
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" size="xs" />}>
            <Icon name="calendar" />
            {RANGES.find((item) => item.value === range)?.label}
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="min-w-40">
                <Menu.RadioGroup value={range} onValueChange={(value: MetricRange) => setRange(value)}>
                  {RANGES.map((item) => (
                    <Menu.RadioItem key={item.value} value={item.value}>
                      {item.label}
                    </Menu.RadioItem>
                  ))}
                </Menu.RadioGroup>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {charts.map((chart) => (
          <MetricChart
            key={chart.key}
            title={chart.title}
            value={chart.value}
            detail={chart.detail}
            isValueCritical={chart.isValueCritical}
            series={chart.series}
            times={times}
            from={from}
            to={to}
            bucketMs={metrics?.bucket_ms ?? 10_000}
            max={chart.max}
            maxLabel={chart.maxLabel}
            formatValue={chart.formatValue}
            limit={chart.limit}
            legend={chart.legend && <ChartLegend items={chart.legend} />}
          />
        ))}
      </div>
    </section>
  );
}
