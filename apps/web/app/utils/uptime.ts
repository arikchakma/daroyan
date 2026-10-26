import type { Schemas } from '@daroyan/client';

export const INTERVAL_OPTIONS = [
  { value: 30, label: '30 seconds' },
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 600, label: '10 minutes' },
  { value: 1800, label: '30 minutes' },
  { value: 3600, label: '1 hour' },
];

export const TIMEOUT_OPTIONS = [
  { value: 5, label: '5 seconds' },
  { value: 10, label: '10 seconds' },
  { value: 15, label: '15 seconds' },
  { value: 30, label: '30 seconds' },
];

export function formatEvery(secs: number) {
  if (secs < 60) {
    return `${secs}s`;
  }
  if (secs < 3600) {
    return `${secs / 60}m`;
  }
  return `${secs / 3600}h`;
}

export function formatInterval(secs: number) {
  return INTERVAL_OPTIONS.find((option) => option.value === secs)?.label ?? `${secs} seconds`;
}

export function formatUptime(ratio: number | null | undefined) {
  if (ratio === null || ratio === undefined) {
    return '—';
  }
  const percent = ratio * 100;
  return percent === 100 ? '100%' : `${Math.min(percent, 99.99).toFixed(2)}%`;
}

export function formatMs(ms: number | null | undefined) {
  if (ms === null || ms === undefined) {
    return '—';
  }
  return ms >= 1000 ? `${(ms / 1000).toFixed(ms >= 10_000 ? 0 : 1)} s` : `${Math.round(ms)} ms`;
}

export function describeTarget(check: Pick<Schemas['UptimeCheck'], 'kind' | 'method' | 'target'>) {
  return check.kind === 'tcp' ? `TCP ${check.target}` : `${check.method} ${check.target}`;
}

export function describeExpect(check: Schemas['UptimeCheck']) {
  if (check.kind === 'tcp') {
    return 'Port open';
  }
  return check.expected_status ? `Status ${check.expected_status}` : 'Any 2xx or 3xx';
}

export function getUptime(days: Schemas['UptimeDay'][]) {
  const total = days.reduce((sum, day) => sum + day.total, 0);
  const down = days.reduce((sum, day) => sum + day.down, 0);
  return total > 0 ? (total - down) / total : null;
}
