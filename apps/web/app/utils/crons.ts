import type { Schemas } from '@daroyan/client';

import { formatClock, formatUnit } from '~/utils/format';

export const GRACE_OPTIONS = [
  { value: 60, label: '1 minute' },
  { value: 120, label: '2 minutes' },
  { value: 300, label: '5 minutes' },
  { value: 600, label: '10 minutes' },
  { value: 1800, label: '30 minutes' },
  { value: 3600, label: '1 hour' },
];

export const MAX_DURATION_OPTIONS = [
  { value: 0, label: 'No limit' },
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 600, label: '10 minutes' },
  { value: 1800, label: '30 minutes' },
  { value: 3600, label: '1 hour' },
  { value: 21_600, label: '6 hours' },
  { value: 86_400, label: '24 hours' },
];

export const INTERVAL_OPTIONS = [
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
  { value: 1800, label: '30 minutes' },
  { value: 3600, label: '1 hour' },
  { value: 21_600, label: '6 hours' },
  { value: 43_200, label: '12 hours' },
  { value: 86_400, label: '1 day' },
  { value: 604_800, label: '7 days' },
];

export const COMMON_SCHEDULES = [
  { label: 'Every 5 minutes', expression: '*/5 * * * *' },
  { label: 'Every hour', expression: '0 * * * *' },
  { label: 'Daily at 02:00', expression: '0 2 * * *' },
  { label: 'Weekdays at 09:00', expression: '0 9 * * 1-5' },
  { label: 'Weekly', expression: '0 0 * * 0' },
  { label: 'Monthly', expression: '0 0 1 * *' },
];

export function formatSecs(secs: number) {
  const units: [number, 'day' | 'hour' | 'minute'][] = [
    [86_400, 'day'],
    [3600, 'hour'],
    [60, 'minute'],
  ];
  const [size, unit] = units.find(([size]) => secs >= size && secs % size === 0) ?? [1, 'second' as const];
  return formatUnit(secs / size, unit);
}

export function describeSchedule(cron: Pick<Schemas['Cron'], 'schedule_kind' | 'expression' | 'interval_secs'>) {
  return cron.schedule_kind === 'every' ? `every ${formatSecs(cron.interval_secs ?? 0)}` : (cron.expression ?? '');
}

export function getPingUrl(pingKey: string) {
  return `${window.location.origin}/p/${pingKey}`;
}

export function formatRunTime(ms: number | null | undefined) {
  if (ms === null || ms === undefined) {
    return '—';
  }
  if (ms < 1000) {
    return `${Math.round(ms)} ms`;
  }
  const seconds = ms / 1000;
  if (seconds < 60) {
    return `${seconds.toFixed(seconds >= 10 ? 0 : 1)} s`;
  }
  const minutes = Math.floor(seconds / 60);
  return minutes < 60 ? `${minutes}m ${Math.round(seconds % 60)}s` : `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function formatLateness(run: Pick<Schemas['CronRun'], 'scheduled_at' | 'started_at' | 'finished_at'>) {
  if (run.scheduled_at === null || run.scheduled_at === undefined) {
    return '—';
  }
  const delay = (run.started_at ?? run.finished_at) - run.scheduled_at;
  return delay < 1000 ? 'On time' : `${formatRunTime(delay)} after the schedule`;
}

export function joinTimes(times: number[]) {
  const clocks = times.map(formatClock);
  return clocks.length > 1 ? `${clocks.slice(0, -1).join(', ')} and ${clocks.at(-1)}` : (clocks[0] ?? '');
}

export function formatOutputSize(output: string) {
  const bytes = new TextEncoder().encode(output).length;
  return bytes < 1000 ? `${bytes} B` : `${(bytes / 1000).toFixed(1)} kB`;
}

const UNTIL_FORMAT = new Intl.RelativeTimeFormat('en', { numeric: 'always' });

export function formatUntil(at: number, now = Date.now()) {
  const minutes = Math.max(Math.round((at - now) / 60_000), 1);
  if (minutes < 60) {
    return UNTIL_FORMAT.format(minutes, 'minute');
  }
  const hours = Math.round(minutes / 60);
  return hours < 48 ? UNTIL_FORMAT.format(hours, 'hour') : UNTIL_FORMAT.format(Math.round(hours / 24), 'day');
}
