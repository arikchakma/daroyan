import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { Sparkline } from '~/components/servers/sparkline';
import { UsageRing } from '~/components/servers/usage-ring';
import type { UsageTone } from '~/components/servers/usage-ring';
import { serverMetricsOptions } from '~/queries/servers';
import { cn } from '~/utils/classname';
import { formatBytes, formatDate, formatDuration, formatPercent, formatUsedOf, getPercent } from '~/utils/format';

function usageTone(percent: number): UsageTone {
  return percent >= 90 ? 'crit' : percent >= 75 ? 'warn' : 'ok';
}

type SpecGridProps = {
  serverId: string;
  host: Schemas['Host'];
  report: Schemas['LastReport'];
};

export function SpecGrid(props: SpecGridProps) {
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
  const loadTone: UsageTone = loadRatio >= 1 ? 'crit' : loadRatio >= 0.75 ? 'warn' : 'ok';
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
  ring?: { percent: number; tone: UsageTone };
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
