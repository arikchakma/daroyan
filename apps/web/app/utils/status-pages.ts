import type { Schemas } from '@daroyan/client';

import type { IconName } from '~/components/icons/icon-paths';

type PublicComponent = Schemas['PublicStatusComponent'];
type PublicDay = Schemas['UptimeDay'];

export type ComponentRef = Schemas['StatusPageComponent'];

export type Monitor = {
  kind: string;
  id: string;
  name: string;
  caption: string;
  icon: IconName;
};

export const MONITOR_GROUPS = [
  { kind: 'check', label: 'Uptime Checks' },
  { kind: 'cron', label: 'Crons' },
];

export function getMonitorKey(monitor: Pick<ComponentRef, 'kind' | 'id'>) {
  return `${monitor.kind}:${monitor.id}`;
}

export const PAGE_STATUS = {
  operational: { label: 'Operational', tone: 'success', icon: 'circle-check' },
  partial: { label: 'Partial Outage', tone: 'warning', icon: 'triangle-alert' },
  major: { label: 'Major Outage', tone: 'critical', icon: 'circle-x' },
  maintenance: { label: 'Under Maintenance', tone: 'info', icon: 'calendar' },
} as const;

export const COMPONENT_STATUS = {
  operational: { label: 'Operational', className: 'text-tone-success-fg' },
  degraded: { label: 'Degraded Performance', className: 'text-tone-warning-fg' },
  partial: { label: 'Partial Outage', className: 'text-tone-warning-fg' },
  outage: { label: 'Major Outage', className: 'text-tone-critical-fg' },
  maintenance: { label: 'Under Maintenance', className: 'text-tone-info-fg' },
  unknown: { label: 'No data', className: 'text-text-secondary' },
} as const;

export function getPageStatus(status: string) {
  return PAGE_STATUS[status as keyof typeof PAGE_STATUS] ?? PAGE_STATUS.operational;
}

export function getComponentStatus(status: string) {
  return COMPONENT_STATUS[status as keyof typeof COMPONENT_STATUS] ?? COMPONENT_STATUS.unknown;
}

export function getPublicUrl(slug: string) {
  return `${window.location.origin}/s/${slug}`;
}

export function joinNames(names: string[]) {
  return names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : (names[0] ?? '');
}

export function summarizeComponents(components: PublicComponent[]) {
  const down = components
    .filter((component) => component.status === 'outage' || component.status === 'partial')
    .map((component) => component.name);
  const slow = components.filter((component) => component.status === 'degraded');
  const maintained = components
    .filter((component) => component.status === 'maintenance')
    .map((component) => component.name);
  const degraded = slow.map((component) => component.name);
  const isLate = slow.every((component) => component.kind === 'cron');
  const parts = [];
  if (down.length > 0) {
    parts.push(`${joinNames(down)} ${down.length === 1 ? 'is' : 'are'} down`);
  }
  if (degraded.length > 0) {
    parts.push(`${joinNames(degraded)} ${degraded.length === 1 ? 'is' : 'are'} ${isLate ? 'running late' : 'degraded'}`);
  }
  if (maintained.length > 0) {
    parts.push(`${joinNames(maintained)} ${maintained.length === 1 ? 'is' : 'are'} under maintenance`);
  }
  return parts.length > 0 ? `${parts.join(' and ')}.` : 'All systems are running normally.';
}

export function getDayClassName(day: PublicDay) {
  if (day.total === 0) {
    return 'bg-surface-fill';
  }
  if (day.down > 0) {
    return 'bg-delta-bad';
  }
  if (day.slow > 0) {
    return 'bg-tone-warning-fg';
  }
  return 'bg-tone-success-solid';
}

export function describeDayIssues(day: PublicDay, kind: string) {
  if (day.total === 0 || (day.down === 0 && day.slow === 0)) {
    return null;
  }
  const unit = kind === 'cron' ? 'run' : 'check';
  const parts = [];
  if (day.down > 0) {
    parts.push(`${day.down} of ${day.total} ${unit}s failed`);
  }
  if (day.slow > 0) {
    parts.push(`${day.slow} ${kind === 'cron' ? 'late' : 'slow'}`);
  }
  return parts.join(', ');
}
