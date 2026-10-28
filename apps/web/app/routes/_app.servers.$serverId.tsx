import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { AlertBadge, AlertBanners } from '~/components/alerts/alert-banners';
import { Icon } from '~/components/icons/icon';
import { ChartLegend, getNiceMax, MetricChart } from '~/components/servers/metric-chart';
import type { ChartSeries } from '~/components/servers/metric-chart';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { DataTable } from '~/components/ui/data-table';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useFilesystemsTable } from '~/hooks/use-filesystems-table';
import { usePortsTable } from '~/hooks/use-ports-table';
import { useProcessesTable } from '~/hooks/use-processes-table';
import { ServerStatusBadge } from '~/components/servers/server-status-badge';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { Dialog } from '~/components/ui/dialog';
import { Menu } from '~/components/ui/menu';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  deleteServerOptions,
  renameServerOptions,
  serverEventsOptions,
  serverMetricsOptions,
  serverOptions,
} from '~/queries/servers';
import type { MetricRange } from '~/queries/servers';
import { cn } from '~/utils/classname';
import {
  formatBytes,
  formatCount,
  formatDate,
  formatDuration,
  formatEventTime,
  formatPercent,
  formatRelative,
  formatSpeed,
  formatUsedOf,
  getPercent,
} from '~/utils/format';

import type { Route } from './+types/_app.servers.$serverId';

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

export default function ServerPage(props: Route.ComponentProps) {
  const { serverId } = props.params;

  const { data: server, error } = useQuery(serverOptions(serverId));

  if (error) {
    return (
      <div className="flex h-full flex-col bg-surface-page">
        <TopBar serverId={serverId} />
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-20">
          <p className="text-title-section font-medium text-text-primary">Server Not Found</p>
          <p className="text-body text-text-secondary">It may have been removed.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <TopBar serverId={serverId} serverName={server?.name} />
      {server && <ServerBody server={server} />}
    </div>
  );
}

type TopBarProps = {
  serverId: string;
  serverName?: string;
};

function TopBar(props: TopBarProps) {
  const { serverId, serverName } = props;

  const navigate = useNavigate();
  const isAdmin = useIsAdmin();
  const { mutate: deleteServer } = useMutation({
    ...deleteServerOptions(),
    meta: { errorMessage: 'The server could not be removed' },
  });

  const { mutate: renameServer } = useMutation({
    ...renameServerOptions(),
    meta: { errorMessage: 'The server could not be renamed' },
  });

  const rename = () => {
    const name = window.prompt('Server name', serverName ?? '')?.trim();
    if (!name || name === serverName) {
      return;
    }
    renameServer(
      { serverId, name },
      {
        onSuccess: () => {
          queryClient.invalidateQueries({ queryKey: ['servers'] });
          toast.success(`Renamed to ${name}`);
        },
      }
    );
  };

  const remove = () => {
    if (!window.confirm(`Remove ${serverName ?? 'this server'}? Its history is deleted too.`)) {
      return;
    }
    deleteServer(
      { serverId },
      {
        onSuccess: () => {
          queryClient.invalidateQueries({ queryKey: ['servers'] });
          toast.success(`Removed ${serverName ?? 'the server'}`);
          navigate('/servers');
        },
      }
    );
  };

  useCommandActions(serverName ?? 'Server', [
    {
      label: 'View Logs',
      icon: 'terminal',
      onSelect: () => void navigate(`/logs?server_id=${serverId}`),
    },
    isAdmin && !!serverName && { label: 'Rename Server', icon: 'pencil', onSelect: rename },
    isAdmin && !!serverName && { label: 'Remove Server', icon: 'trash', onSelect: remove },
  ]);

  return (
    <PageBar backTo="/servers" icon="server" title="Servers">
      {serverName && (
        <>
          {isAdmin && (
            <Menu.Root>
              <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
                More Actions
                <Icon name="chevron-down" />
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Positioner align="end">
                  <Menu.Popup>
                    <Menu.Item onClick={rename}>
                      <Icon name="pencil" />
                      Rename Server
                    </Menu.Item>
                    <Menu.Item onClick={remove} className="text-tone-critical-fg">
                      <Icon name="trash" />
                      Remove Server
                    </Menu.Item>
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.Root>
          )}
          <Button nativeButton={false} render={<Link to={`/logs?server_id=${serverId}`} />} className="w-fit">
            <Icon name="terminal" />
            View Logs
          </Button>
        </>
      )}
    </PageBar>
  );
}

type ServerBodyProps = {
  server: Schemas['ServerDetail'];
};

function ServerBody(props: ServerBodyProps) {
  const { server } = props;

  const host = server.host;
  const report = server.last_report;
  const subtitle = [host?.hostname, 'reports every 10s'].filter(Boolean).join(' · ');

  return (
    <div className="flex flex-1 justify-center px-10 pt-3 pb-12">
      <div className="flex w-full max-w-240 min-w-0 flex-col gap-6">
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2.5">
            <h1 className="truncate text-title-page font-semibold text-text-primary">{server.name}</h1>
            {server.status === 'down' ? (
              <ServerStatusBadge status={server.status} />
            ) : (
              <AlertBadge serverId={server.id} fallback={<ServerStatusBadge status={server.status} />} />
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2.5">
            <p className="text-body text-text-secondary">{subtitle}</p>
            {server.labels.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {server.labels.map((label) => (
                  <span
                    key={label}
                    className="flex h-5.5 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary"
                  >
                    {label}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-4 lg:flex-row">
          {host && report ? (
            <SpecGrid serverId={server.id} host={host} report={report} />
          ) : (
            <WaitingNotice />
          )}
          <EventsCard serverId={server.id} serverName={server.name} />
        </div>

        {server.status === 'down' && (
          <div className="flex items-start gap-3 rounded-xl border border-tone-critical-bg bg-tone-critical-surface px-4 py-3.5">
            <Icon name="triangle-alert" className="mt-0.5 text-tone-critical-fg" />
            <div className="flex flex-col gap-0.5">
              <p className="text-body font-medium text-tone-critical-fg">{server.name} is not connected</p>
              <p className="text-body text-text-primary">
                The last report came {server.last_seen_at ? formatRelative(server.last_seen_at) : 'never'}. Check the
                agent on the server with <code className="font-mono text-code">systemctl status daroyan</code>.
              </p>
            </div>
          </div>
        )}

        <AlertBanners serverId={server.id} />

        {host && <MetricsSection serverId={server.id} host={host} report={report} />}
        {report && <ProcessesSection serverId={server.id} processes={report.processes} />}
        {report && <FilesystemsSection disks={report.state.disks} />}
        {report && <PortsSection ports={report.ports} />}
      </div>
    </div>
  );
}

function WaitingNotice() {
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3 rounded-2xl border border-border-ring bg-surface-default px-4 py-3.5 shadow-card">
      <Icon name="loader" className="animate-spin text-icon-secondary" />
      <div className="flex flex-1 flex-col gap-0.5">
        <p className="text-body font-medium text-text-primary">Waiting for the first report</p>
        <p className="text-body text-text-secondary">The agent sends one every 10 seconds once it connects.</p>
      </div>
    </div>
  );
}

type Tone = 'ok' | 'warn' | 'crit';

function usageTone(percent: number): Tone {
  return percent >= 90 ? 'crit' : percent >= 75 ? 'warn' : 'ok';
}

const RING_CLASS: Record<Tone, string> = {
  ok: 'stroke-chart-primary',
  warn: 'stroke-toast-warning-icon',
  crit: 'stroke-delta-bad',
};

function UsageRing(props: { percent: number; tone: Tone }) {
  const clamped = Math.max(0, Math.min(props.percent, 100));
  return (
    <svg viewBox="0 0 44 44" className="size-7 shrink-0 -rotate-90" aria-hidden="true">
      <circle cx="22" cy="22" r="18" fill="none" strokeWidth="5" className="stroke-surface-fill-strong" />
      <circle
        cx="22"
        cy="22"
        r="18"
        fill="none"
        strokeWidth="5"
        strokeLinecap="round"
        pathLength={100}
        strokeDasharray={`${clamped} 100`}
        className={RING_CLASS[props.tone]}
      />
    </svg>
  );
}

function Sparkline(props: { values: number[] }) {
  const { values } = props;
  if (values.length < 2) {
    return null;
  }

  const width = 60;
  const height = 32;
  const max = Math.max(...values);
  const min = Math.min(...values);
  const range = max - min || 1;
  const step = width / (values.length - 1);
  const coords = values.map((value, index) => [index * step, height - ((value - min) / range) * height] as const);
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

type SpecGridProps = {
  serverId: string;
  host: Schemas['Host'];
  report: Schemas['LastReport'];
};

function SpecGrid(props: SpecGridProps) {
  const { serverId, host, report } = props;

  const { data: metrics } = useQuery(serverMetricsOptions(serverId, '1h'));
  const points = metrics?.points ?? [];
  const loadSeries = points.map((point) => point.load1);
  const trafficSeries = points.map((point) => point.net_in_speed + point.net_out_speed);

  const state = report.state;
  const diskUsed = state.disks.reduce((sum, disk) => sum + disk.disk_used, 0);
  const diskTotal = state.disks.reduce((sum, disk) => sum + disk.disk_total, 0);
  const transferIn = state.nics.reduce((sum, nic) => sum + nic.net_in_transfer, 0);
  const transferOut = state.nics.reduce((sum, nic) => sum + nic.net_out_transfer, 0);

  const cpuPercent = state.cpu;
  const memPercent = getPercent(state.mem_used, host.mem_total);
  const diskPercent = getPercent(diskUsed, diskTotal);
  const loadRatio = host.cpu_cores ? state.load1 / host.cpu_cores : 0;
  const loadTone: Tone = loadRatio >= 1 ? 'crit' : loadRatio >= 0.75 ? 'warn' : 'ok';
  const cpuTone = usageTone(cpuPercent);
  const memTone = usageTone(memPercent);
  const diskTone = usageTone(diskPercent);

  const vitals: SpecProps[] = [
    { label: 'Uptime', value: formatDuration(state.uptime), detail: `since ${formatDate(host.boot_time * 1000)}` },
    { label: 'CPU', value: formatPercent(cpuPercent), detail: `${host.cpu_cores} vCPU`, ring: { percent: cpuPercent, tone: cpuTone }, valueCritical: cpuTone === 'crit' },
    { label: 'Memory', value: formatPercent(memPercent), detail: formatUsedOf(state.mem_used, host.mem_total), ring: { percent: memPercent, tone: memTone }, valueCritical: memTone === 'crit' },
    { label: 'Disk', value: formatPercent(diskPercent), detail: formatUsedOf(diskUsed, diskTotal), ring: { percent: diskPercent, tone: diskTone }, valueCritical: diskTone === 'crit' },
    { label: 'Load', value: state.load1.toFixed(2), detail: `${host.cpu_cores} cores`, spark: loadSeries, valueCritical: loadTone === 'crit' },
    { label: 'Traffic', value: formatBytes(transferIn + transferOut), detail: `↑ ${formatBytes(transferOut)}`, spark: trafficSeries },
  ];

  const details: { label: string; value: string }[] = [
    { label: 'OS', value: [host.platform, host.platform_version].filter(Boolean).join(' ') || '—' },
    { label: 'Kernel', value: host.kernel || '—' },
    { label: 'Arch', value: host.arch || '—' },
    { label: 'Hostname', value: host.hostname || '—' },
    { label: 'Private IP', value: host.private_ip ?? '—' },
    { label: 'Agent', value: host.version || '—' },
  ];

  return (
    <div className="min-w-0 flex-1 overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
      <div className="grid grid-cols-2 gap-px bg-border-row md:grid-cols-3">
        {vitals.map((spec) => (
          <Spec key={spec.label} {...spec} />
        ))}
      </div>
      <details className="group border-t border-border-row">
        <summary className="flex cursor-pointer list-none items-center gap-2 px-4 py-2.5 text-caption font-medium text-text-secondary select-none">
          <Icon name="info-fill" className="text-icon-secondary" />
          System details
          <Icon name="chevron-down" className="ml-auto text-icon-secondary transition-transform group-open:rotate-180" />
        </summary>
        <div className="grid grid-cols-2 gap-px border-t border-border-row bg-border-row md:grid-cols-3">
          {details.map((item) => (
            <div key={item.label} className="flex min-w-0 flex-col gap-1 bg-surface-default px-4 py-3">
              <span className="text-caption font-medium text-text-secondary">{item.label}</span>
              <span className="min-w-0 truncate font-mono text-code text-text-primary">{item.value}</span>
            </div>
          ))}
        </div>
      </details>
    </div>
  );
}

type SpecProps = {
  label: string;
  value: string;
  detail?: string;
  ring?: { percent: number; tone: Tone };
  spark?: number[];
  valueCritical?: boolean;
};

function Spec(props: SpecProps) {
  const { label, value, detail, ring, spark, valueCritical } = props;

  return (
    <div className="flex min-w-0 items-center justify-between gap-3 bg-surface-default px-4 py-3.5">
      <div className="flex min-w-0 flex-col gap-1">
        <span className="text-caption font-medium text-text-secondary">{label}</span>
        <div className="flex min-w-0 items-center gap-1.5 whitespace-nowrap">
          <span className={cn('min-w-0 truncate text-body font-semibold', valueCritical ? 'text-tone-critical-fg' : 'text-text-primary')}>
            {value}
          </span>
          {detail && <span className="min-w-0 shrink-1000 truncate text-caption font-normal text-text-tertiary">{detail}</span>}
        </div>
      </div>
      {ring && <UsageRing percent={ring.percent} tone={ring.tone} />}
      {spark && <Sparkline values={spark} />}
    </div>
  );
}

type MetricsSectionProps = {
  serverId: string;
  host: Schemas['Host'];
  report?: Schemas['LastReport'] | null;
};

function MetricsSection(props: MetricsSectionProps) {
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

type ProcessesSectionProps = {
  serverId: string;
  processes: Schemas['ProcessState'][];
};

function ProcessesSection(props: ProcessesSectionProps) {
  const { serverId, processes } = props;

  const table = useProcessesTable({ serverId, processes });
  const running = processes.filter((process) => process.status === 'running').length;

  return (
    <DetailSection title="Processes" summary={processes.length > 0 ? `${running} of ${processes.length} running` : undefined}>
      <DetailCard>
        {processes.length > 0 ? (
          <DataTable table={table} label="Processes" variant="card" size="lg" />
        ) : (
          <DetailEmpty>
            No watched processes. Add one under <code className="font-mono text-code">processes</code> in the agent
            config.
          </DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}

type FilesystemsSectionProps = {
  disks: Schemas['DiskState'][];
};

function FilesystemsSection(props: FilesystemsSectionProps) {
  const { disks } = props;

  const table = useFilesystemsTable(disks);
  const full = disks.filter((disk) => getPercent(disk.disk_used, disk.disk_total) >= 90).length;

  return (
    <DetailSection
      title="Filesystems"
      summary={disks.length > 0 ? (full > 0 ? `${full} almost full` : `${disks.length} OK`) : undefined}
    >
      <DetailCard>
        {disks.length > 0 ? (
          <DataTable table={table} label="Filesystems" variant="card" size="lg" />
        ) : (
          <DetailEmpty>The agent reported no filesystems.</DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}

type PortsSectionProps = {
  ports: Schemas['PortState'][];
};

function PortsSection(props: PortsSectionProps) {
  const { ports } = props;

  const table = usePortsTable(ports);
  const open = ports.filter((port) => port.open).length;

  return (
    <DetailSection title="Ports and Hosts" summary={ports.length > 0 ? `${open} open` : undefined}>
      <DetailCard>
        {ports.length > 0 ? (
          <DataTable table={table} label="Ports and Hosts" variant="card" size="lg" />
        ) : (
          <DetailEmpty>
            No watched ports. Add one under <code className="font-mono text-code">ports</code> in the agent config.
          </DetailEmpty>
        )}
      </DetailCard>
    </DetailSection>
  );
}

function getEventText(event: Schemas['EventItem']) {
  const data = event.data as Record<string, string | number | null | undefined>;
  switch (event.kind) {
    case 'agent_started':
      return { text: `Agent ${data.version ?? ''} started`, tone: 'default' as const };
    case 'agent_connected':
      return { text: 'Agent connected', tone: 'default' as const };
    case 'agent_disconnected':
      return { text: 'Agent disconnected', tone: 'critical' as const };
    case 'process_exited':
      return {
        text: data.code === null || data.code === undefined ? `${data.name} stopped` : `${data.name} exited ${data.code}`,
        tone: 'critical' as const,
      };
    case 'process_changed':
      return { text: `${data.name} restarted outside Daroyan (PID ${data.old_pid} → ${data.new_pid})`, tone: 'warning' as const };
    case 'process_restarted':
      return { text: `${data.name} restarted`, tone: 'warning' as const };
    case 'process_restart_failed':
      return { text: `${data.name} restart failed`, tone: 'critical' as const };
    case 'process_restart_gave_up':
      return { text: `${data.name} still down after ${data.attempts} restarts`, tone: 'critical' as const };
    default:
      return { text: event.kind, tone: 'default' as const };
  }
}

const VISIBLE_EVENTS = 4;

type EventRowProps = {
  event: Schemas['EventItem'];
};

function EventRow(props: EventRowProps) {
  const { event } = props;

  const { text, tone } = getEventText(event);

  return (
    <li className="flex h-8.5 shrink-0 items-center gap-3 border-t border-border-row px-4 first:border-t-0">
      <span
        className={cn(
          'min-w-0 flex-1 truncate text-body text-text-primary',
          tone === 'critical' && 'text-tone-critical-fg',
          tone === 'warning' && 'text-tone-warning-fg'
        )}
      >
        {text}
      </span>
      <span className="shrink-0 font-mono text-code-small text-text-tertiary">{formatEventTime(event.at)}</span>
    </li>
  );
}

type EventsCardProps = {
  serverId: string;
  serverName: string;
};

function EventsCard(props: EventsCardProps) {
  const { serverId, serverName } = props;

  const { data: events } = useQuery(serverEventsOptions(serverId));

  return (
    <div className="flex w-full shrink-0 flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card lg:w-85">
      <header className="flex items-center px-4 pt-2.5 pb-1">
        <h2 className="flex-1 text-caption font-medium text-text-primary">Events</h2>
        {events && events.length > 0 && (
          <Dialog.Root>
            <Dialog.Trigger className="rounded-xs text-body text-text-primary outline-none hover:underline focus-visible:underline">
              View All Events
            </Dialog.Trigger>
            <Dialog.Portal>
              <Dialog.Backdrop />
              <Dialog.Popup>
                <Dialog.Header>
                  <Dialog.Title>Events</Dialog.Title>
                  <Dialog.Description className="text-text-secondary">
                    The latest {events.length} events from {serverName}.
                  </Dialog.Description>
                </Dialog.Header>
                <ul className="max-h-[min(480px,70vh)] overflow-y-auto pb-2">
                  {events.map((event, index) => (
                    <EventRow key={`${event.at}-${index}`} event={event} />
                  ))}
                </ul>
              </Dialog.Popup>
            </Dialog.Portal>
          </Dialog.Root>
        )}
      </header>
      {events?.length === 0 && <p className="flex h-8.5 items-center px-4 text-body text-text-secondary">No events yet.</p>}
      <ul>
        {events?.slice(0, VISIBLE_EVENTS).map((event, index) => (
          <EventRow key={`${event.at}-${index}`} event={event} />
        ))}
      </ul>
    </div>
  );
}
