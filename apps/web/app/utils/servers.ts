import type { Schemas } from '@daroyan/client';

import type { SpecProps } from '~/components/servers/spec';
import type { UsageTone } from '~/components/servers/usage-ring';
import { formatBytes, formatCoresOf, formatPercent, formatUptimeDays, formatUsedOf, getPercent } from '~/utils/format';

type ServerItem = Schemas['ServerItem'];

export const CRITICAL_PERCENT = 90;
const WARNING_PERCENT = 75;

export type ServersLayout = 'list' | 'grid';

export const LAYOUT_KEY = 'daroyan.servers.layout';

export function isServerLive(server: ServerItem) {
  return server.status === 'healthy' || server.status === 'warning';
}

export function getOsLabel(platform?: string | null, version?: string | null) {
  return [platform, version].filter(Boolean).join(' ');
}

export function getServerMeta(server: ServerItem) {
  return [
    server.private_ip,
    getOsLabel(server.platform, server.platform_version),
    isServerLive(server) && server.uptime ? `up ${formatUptimeDays(server.uptime)}` : null,
  ]
    .filter(Boolean)
    .join(' · ');
}

export function getServerSubtitle(server: ServerItem) {
  return [
    server.private_ip,
    getOsLabel(server.platform, server.platform_version),
    server.cpu_cores && server.mem_total ? `${server.cpu_cores} vCPU, ${formatBytes(server.mem_total)}` : null,
  ]
    .filter(Boolean)
    .join(' · ');
}

export function getPendingHint(isAdmin: boolean) {
  if (isAdmin) {
    return 'Run the install command on the server to see CPU, memory and disk.';
  }

  return 'CPU, memory and disk show up once the server reports.';
}

export function getUsageTone(percent: number): UsageTone {
  if (percent >= CRITICAL_PERCENT) {
    return 'crit';
  }
  if (percent >= WARNING_PERCENT) {
    return 'warn';
  }

  return 'ok';
}

export function isUsageCritical(percent: number) {
  return getUsageTone(percent) === 'crit';
}

export function getLoadTone(load: number, cores: number): UsageTone {
  if (!cores) {
    return 'ok';
  }
  if (load > cores) {
    return 'crit';
  }
  if (load >= cores * 0.75) {
    return 'warn';
  }

  return 'ok';
}

export function getDiskPercent(disk: Schemas['DiskState']) {
  return getPercent(disk.disk_used, disk.disk_total);
}

export function getDiskTotals(disks: Schemas['DiskState'][]) {
  const used = disks.reduce((sum, disk) => sum + disk.disk_used, 0);
  const total = disks.reduce((sum, disk) => sum + disk.disk_total, 0);

  return { used, total, percent: getPercent(used, total) };
}

export function getTransferTotals(nics: Schemas['NicState'][]) {
  return {
    in: nics.reduce((sum, nic) => sum + nic.net_in_transfer, 0),
    out: nics.reduce((sum, nic) => sum + nic.net_out_transfer, 0),
  };
}

export function getUsageSpec(label: string, percent: number | null | undefined, detail: string): SpecProps {
  if (percent === null || percent === undefined) {
    return { label, value: '—', detail };
  }

  const tone = getUsageTone(percent);

  return { label, value: formatPercent(percent), detail, ring: { percent, tone }, valueCritical: isUsageCritical(percent) };
}

function getDiskSpec(server: ServerItem) {
  const total = server.disk_total ?? 0;
  if (!total) {
    return getUsageSpec('Disk', null, 'no disks');
  }
  if (server.status === 'down') {
    return getUsageSpec('Disk', null, formatBytes(total));
  }

  return getUsageSpec('Disk', server.disk_percent, formatUsedOf(server.disk_used ?? 0, total));
}

export function getServerSpecs(server: ServerItem): SpecProps[] {
  const cores = server.cpu_cores ?? 0;
  const memTotal = server.mem_total ?? 0;

  if (server.status === 'down') {
    return [
      getUsageSpec('CPU', null, `${cores} vCPU`),
      getUsageSpec('Memory', null, formatBytes(memTotal)),
      getDiskSpec(server),
    ];
  }

  return [
    getUsageSpec('CPU', server.cpu, formatCoresOf(server.cpu ?? 0, cores)),
    getUsageSpec('Memory', server.mem_percent, formatUsedOf(server.mem_used ?? 0, memTotal)),
    getDiskSpec(server),
  ];
}
