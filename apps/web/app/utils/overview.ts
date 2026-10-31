import type { Schemas } from '@daroyan/client';

import { getAlertTargetPath } from '~/utils/alerts';
import { formatClock } from '~/utils/format';
import { OTLP_SERVER_ID } from '~/utils/logs';

export function getAlertLink(alert: Schemas['Alert']) {
  if (alert.server_id === OTLP_SERVER_ID) {
    return { href: getAlertTargetPath(alert), action: 'Open Logs' };
  }
  if (alert.server_id) {
    return { href: `/servers/${alert.server_id}`, action: 'Open Server' };
  }
  if (alert.check_id) {
    return { href: `/uptime/${alert.check_id}`, action: 'Open Check' };
  }
  if (alert.cron_id) {
    return { href: `/crons/${alert.cron_id}`, action: 'Open Cron' };
  }
  return { href: `/alerts/${alert.rule_id}`, action: 'Open Alert' };
}

export function joinParts(parts: string[]) {
  return parts.length <= 1 ? (parts[0] ?? '') : `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}`;
}

export function getWorstMetric(server: Schemas['ServerItem']) {
  const metrics = [
    { label: 'CPU', value: server.cpu },
    { label: 'Memory', value: server.mem_percent },
    { label: 'Disk', value: server.disk_percent },
  ].filter((metric): metric is { label: string; value: number } => typeof metric.value === 'number');
  const worst = metrics.sort((a, b) => b.value - a.value)[0];
  return worst ? `${worst.label} ${Math.round(worst.value)}%` : '';
}

export function describeCpu(points: Schemas['TrendPoint'][]) {
  if (points.length === 0) {
    return undefined;
  }

  const latest = points.reduce((a, b) => (b.at > a.at ? b : a));
  const peak = points.reduce((a, b) => (b.value > a.value ? b : a));
  return `CPU ${Math.round(latest.value)}% now, peak ${Math.round(peak.value)}% at ${formatClock(peak.at)}`;
}

export function mergeDays(checks: Schemas['UptimeCheck'][]) {
  const byDay = new Map<number, Schemas['UptimeDay']>();
  for (const day of checks.filter((check) => !check.paused).flatMap((check) => check.days)) {
    const merged = byDay.get(day.day) ?? { day: day.day, down: 0, slow: 0, total: 0 };
    byDay.set(day.day, {
      day: day.day,
      down: merged.down + day.down,
      slow: merged.slow + day.slow,
      total: merged.total + day.total,
    });
  }
  return [...byDay.values()].sort((a, b) => a.day - b.day);
}

export function countCleanDays(days: Schemas['UptimeDay'][]) {
  let count = 0;
  for (const day of [...days].reverse()) {
    if (day.down > 0) {
      break;
    }
    count += 1;
  }
  return count;
}

type GetBarToneOptions = {
  value: number;
  warnAt?: number;
  criticalAt?: number;
};

export function getBarTone(options: GetBarToneOptions) {
  const { value, warnAt, criticalAt } = options;

  if (criticalAt !== undefined && value >= criticalAt) {
    return 'critical';
  }
  if (warnAt !== undefined && value >= warnAt) {
    return 'warning';
  }

  return 'normal';
}
