import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { Spec } from '~/components/servers/spec';
import type { SpecProps } from '~/components/servers/spec';
import { serverQueries } from '~/queries/servers';
import { formatBytes, formatDate, formatDuration, formatUsedOf, getPercent } from '~/utils/format';
import { getDiskTotals, getLoadTone, getOsLabel, getTransferTotals, getUsageSpec } from '~/utils/servers';

type SpecGridProps = {
  serverId: string;
  host: Schemas['Host'];
  report: Schemas['LastReport'];
};

export function SpecGrid(props: SpecGridProps) {
  const { serverId, host, report } = props;

  const { data: metrics } = useQuery(serverQueries.metrics(serverId, '1h'));
  const points = metrics?.points ?? [];
  const loadSeries = points.map((point) => point.load1);
  const trafficSeries = points.map((point) => point.net_in_speed + point.net_out_speed);

  const state = report.state;
  const disk = getDiskTotals(state.disks);
  const transfer = getTransferTotals(state.nics);

  const cpuPercent = state.cpu;
  const memPercent = getPercent(state.mem_used, host.mem_total);
  const loadTone = getLoadTone(state.load1, host.cpu_cores);

  const vitals: SpecProps[] = [
    { label: 'Uptime', value: formatDuration(state.uptime), detail: `since ${formatDate(host.boot_time * 1000)}` },
    getUsageSpec('CPU', cpuPercent, `${host.cpu_cores} vCPU`),
    getUsageSpec('Memory', memPercent, formatUsedOf(state.mem_used, host.mem_total)),
    getUsageSpec('Disk', disk.percent, formatUsedOf(disk.used, disk.total)),
    { label: 'Load', value: state.load1.toFixed(2), detail: `${host.cpu_cores} cores`, spark: loadSeries, valueCritical: loadTone === 'crit' },
    { label: 'Traffic', value: formatBytes(transfer.in + transfer.out), detail: `↑ ${formatBytes(transfer.out)}`, spark: trafficSeries },
  ];

  const details: { label: string; value: string }[] = [
    { label: 'OS', value: getOsLabel(host.platform, host.platform_version) || '—' },
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
