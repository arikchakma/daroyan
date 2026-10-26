import type { Schemas } from '@daroyan/client';

import type { IconName } from '~/components/icons/icon-paths';
import type { BadgeProps } from '~/components/ui/badge';
import { formatClock, formatDate, formatDuration } from '~/utils/format';

type Tone = NonNullable<BadgeProps['tone']>;

export type Incident = Schemas['Incident'];
export type IncidentUpdate = Schemas['IncidentUpdate'];

export const KIND_INCIDENT = 'incident';
export const KIND_MAINTENANCE = 'maintenance';

export const INCIDENT_STATUSES = ['investigating', 'identified', 'monitoring', 'resolved'] as const;
export const MAINTENANCE_STATUSES = ['scheduled', 'in_progress', 'completed'] as const;
export const SEVERITIES = ['minor', 'major', 'critical'] as const;
export const COMPONENT_STATUSES = ['degraded', 'partial', 'outage'] as const;

export const STATUS_META: Record<string, { label: string; tone: Tone; icon: IconName }> = {
  investigating: { label: 'Investigating', tone: 'critical', icon: 'circle-dashed' },
  identified: { label: 'Identified', tone: 'warning', icon: 'circle-dashed' },
  monitoring: { label: 'Monitoring', tone: 'info', icon: 'play' },
  resolved: { label: 'Resolved', tone: 'success', icon: 'check' },
  scheduled: { label: 'Scheduled', tone: 'magic', icon: 'calendar' },
  in_progress: { label: 'In Progress', tone: 'info', icon: 'play' },
  completed: { label: 'Completed', tone: 'success', icon: 'check' },
};

export const SEVERITY_META: Record<string, { label: string; tone: Tone; textClassName: string }> = {
  minor: { label: 'Minor', tone: 'neutral', textClassName: 'text-text-primary' },
  major: { label: 'Major', tone: 'warning', textClassName: 'text-tone-warning-fg' },
  critical: { label: 'Critical', tone: 'critical', textClassName: 'text-tone-critical-fg' },
};

export const COMPONENT_STATUS_META: Record<string, { label: string; className: string }> = {
  operational: { label: 'Operational', className: 'text-tone-success-fg' },
  degraded: { label: 'Degraded Performance', className: 'text-tone-warning-fg' },
  partial: { label: 'Partial Outage', className: 'text-tone-warning-fg' },
  outage: { label: 'Major Outage', className: 'text-tone-critical-fg' },
  maintenance: { label: 'Under Maintenance', className: 'text-tone-info-fg' },
};

export const RELATED_LABELS: Record<string, string> = {
  rule: 'Alert',
  server: 'Server',
  check: 'Uptime Check',
  cron: 'Cron',
};

export function getStatusMeta(status: string) {
  return STATUS_META[status] ?? STATUS_META.investigating!;
}

export function getSeverityMeta(severity: string | null | undefined) {
  return severity ? SEVERITY_META[severity] : undefined;
}

export function getComponentStatusMeta(status: string) {
  return COMPONENT_STATUS_META[status] ?? COMPONENT_STATUS_META.operational!;
}

export function isMaintenance(incident: Pick<Incident, 'kind'>) {
  return incident.kind === KIND_MAINTENANCE;
}

export function isClosed(status: string) {
  return status === 'resolved' || status === 'completed';
}

export function statusesOf(kind: string): readonly string[] {
  return kind === KIND_MAINTENANCE ? MAINTENANCE_STATUSES : INCIDENT_STATUSES;
}

export function getRelatedPath(item: { kind: string; id: string }) {
  switch (item.kind) {
    case 'rule':
      return `/alerts/${item.id}`;
    case 'server':
      return `/servers/${item.id}`;
    case 'check':
      return `/uptime/${item.id}`;
    case 'cron':
      return `/crons/${item.id}`;
    default:
      return undefined;
  }
}

export function getDurationSeconds(incident: Pick<Incident, 'started_at' | 'resolved_at'>, now = Date.now()) {
  const end = incident.resolved_at ? Date.parse(incident.resolved_at) : now;
  return Math.max(Math.round((end - Date.parse(incident.started_at)) / 1000), 0);
}

export function formatSpan(seconds: number) {
  return formatDuration(seconds).replace(/ 0m$/, '');
}

export function formatIncidentDuration(incident: Pick<Incident, 'started_at' | 'resolved_at'>, now = Date.now()) {
  return formatDuration(getDurationSeconds(incident, now));
}

export function formatDay(value: string | number, now = Date.now()) {
  const date = new Date(value);
  const today = new Date(now);
  const yesterday = new Date(now - 86_400_000);
  if (date.toDateString() === today.toDateString()) {
    return 'Today';
  }
  if (date.toDateString() === yesterday.toDateString()) {
    return 'Yesterday';
  }
  return formatDate(value);
}

export function formatMoment(value: string | number, now = Date.now()) {
  const day = formatDay(value, now);
  return `${day}, ${formatClock(new Date(value).getTime())}`;
}

export function formatWindow(startsAt: string, endsAt: string) {
  const start = Date.parse(startsAt);
  const end = Date.parse(endsAt);
  const sameDay = new Date(start).toDateString() === new Date(end).toDateString();
  return sameDay
    ? `${formatDate(start)}, ${formatClock(start)}–${formatClock(end)}`
    : `${formatDate(start)}, ${formatClock(start)} – ${formatDate(end)}, ${formatClock(end)}`;
}

export function formatCountdown(value: string, now = Date.now()) {
  const seconds = Math.round((Date.parse(value) - now) / 1000);
  if (seconds <= 0) {
    return 'now';
  }
  const days = Math.floor(seconds / 86_400);
  if (days >= 1) {
    return days === 1 ? 'in 1 day' : `in ${days} days`;
  }
  return `in ${formatDuration(seconds)}`;
}

export function getInitial(name: string) {
  return name.trim().charAt(0).toUpperCase() || '?';
}

export function joinNames(names: string[]) {
  if (names.length <= 1) {
    return names.join('');
  }
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
}

export function describeShownPages(incident: Pick<Incident, 'status_pages'>) {
  const shown = incident.status_pages.filter((page) => page.is_shown).map((page) => page.name);
  return shown.length > 0 ? `on ${joinNames(shown)}` : 'internal only';
}

export function groupByDay<T extends { created_at: string }>(entries: T[], now = Date.now()) {
  const groups: { day: string; entries: T[] }[] = [];
  for (const entry of entries) {
    const day = formatDay(entry.created_at, now);
    const label = day === 'Today' ? `Today · ${formatDate(entry.created_at)}` : day;
    const last = groups.at(-1);
    if (last && last.day === label) {
      last.entries.push(entry);
    } else {
      groups.push({ day: label, entries: [entry] });
    }
  }
  return groups;
}

export function sortIncidents(incidents: Incident[]) {
  const rank = (incident: Incident) => {
    if (!isClosed(incident.status) && incident.status !== 'scheduled') {
      return 0;
    }
    return incident.status === 'scheduled' ? 1 : 2;
  };
  return [...incidents].sort((a, b) => {
    const order = rank(a) - rank(b);
    if (order !== 0) {
      return order;
    }
    if (a.status === 'scheduled' && b.status === 'scheduled') {
      return Date.parse(a.starts_at ?? a.started_at) - Date.parse(b.starts_at ?? b.started_at);
    }
    return Date.parse(b.started_at) - Date.parse(a.started_at);
  });
}
