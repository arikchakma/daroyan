import type { Schemas } from '@daroyan/client';

import type { SpecProps } from '~/components/servers/spec';
import type { UsageTone } from '~/components/servers/usage-ring';
import { formatPercent, getPercent } from '~/utils/format';

type ServerItem = Schemas['ServerItem'];

export const CRITICAL_PERCENT = 90;
const WARNING_PERCENT = 75;

export function isServerLive(server: ServerItem) {
  return server.status === 'healthy' || server.status === 'warning';
}

export function getOsLabel(platform?: string | null, version?: string | null) {
  return [platform, version].filter(Boolean).join(' ');
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

export function isProcessDegraded(process: Schemas['ProcessState']) {
  const { status, containers } = process;
  return status === 'running' && !!containers && containers.running < containers.min_running;
}
