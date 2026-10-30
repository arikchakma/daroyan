import type { Schemas } from '@daroyan/client';

import type { IconName } from '~/components/icons/icon-paths';
import { formatClock, formatDate } from '~/utils/format';
import { OTLP_SERVER_ID } from '~/utils/logs';

export const CONNECTED_MESSAGE = 'daroyan:integration-connected';

export type ConditionKind = Schemas['Condition']['kind'];
export type IntegrationKind = Schemas['IntegrationConfig']['kind'];
export type Scope = Schemas['Scope'];

export type RuleOptions = Schemas['RuleOptions'];
export type Aggregation = Schemas['Aggregation'];
export type NoData = Schemas['NoData'];

export const METRIC_GROUPS: { label: string; items: { kind: ConditionKind; label: string }[] }[] = [
  {
    label: 'Servers',
    items: [
      { kind: 'cpu', label: 'CPU usage' },
      { kind: 'memory', label: 'Memory usage' },
      { kind: 'disk', label: 'Disk usage' },
      { kind: 'load', label: 'Load (1m)' },
      { kind: 'offline', label: 'Server offline' },
      { kind: 'process_stopped', label: 'Process stopped' },
      { kind: 'port_closed', label: 'Port closed' },
    ],
  },
  {
    label: 'Uptime Checks',
    items: [
      { kind: 'uptime_down', label: 'Status is down' },
      { kind: 'response_time', label: 'Response time' },
      { kind: 'first_byte', label: 'First byte time' },
      { kind: 'cert_days', label: 'Certificate days left' },
    ],
  },
  {
    label: 'Crons',
    items: [
      { kind: 'cron_missed', label: 'Missed a run' },
      { kind: 'cron_failed', label: 'Failed' },
    ],
  },
  { label: 'Logs', items: [{ kind: 'logs', label: 'Matching lines' }] },
];

export const CONDITION_OPTIONS = METRIC_GROUPS.flatMap((group) => group.items);

const CHECK_KINDS: ConditionKind[] = ['uptime_down', 'response_time', 'first_byte', 'cert_days'];
const CRON_KINDS: ConditionKind[] = ['cron_missed', 'cron_failed'];
const AGGREGATED_KINDS: ConditionKind[] = ['cpu', 'memory', 'disk', 'load', 'response_time', 'first_byte'];

export function isCheckKind(kind: ConditionKind) {
  return CHECK_KINDS.includes(kind);
}

export function isCronKind(kind: ConditionKind) {
  return CRON_KINDS.includes(kind);
}

export function hasAggregation(kind: ConditionKind) {
  return AGGREGATED_KINDS.includes(kind);
}

export function getUnit(kind: ConditionKind) {
  switch (kind) {
    case 'cpu':
    case 'memory':
    case 'disk':
      return '%';
    case 'response_time':
    case 'first_byte':
      return 'ms';
    case 'cert_days':
      return 'days';
    case 'logs':
      return 'lines';
    default:
      return '';
  }
}

export const AGGREGATION_OPTIONS: { value: Aggregation; label: string }[] = [
  { value: 'avg', label: 'Average' },
  { value: 'p95', label: 'p95' },
  { value: 'max', label: 'Max' },
  { value: 'min', label: 'Min' },
];

export const WINDOW_OPTIONS: { value: number; label: string }[] = [
  { value: 0, label: 'Latest value' },
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
  { value: 3600, label: '1 hour' },
];

export const RESOLVE_OPTIONS: { value: number; label: string }[] = [
  { value: 0, label: 'As soon as it recovers' },
  { value: 60, label: 'After 1 minute' },
  { value: 300, label: 'After 5 minutes' },
  { value: 900, label: 'After 15 minutes' },
];

export const NO_DATA_OPTIONS: { value: NoData; label: string }[] = [
  { value: 'keep', label: 'Keep the last state' },
  { value: 'alert', label: 'Alert' },
  { value: 'resolve', label: 'Resolve' },
];

export const REMIND_OPTIONS: { value: string; label: string }[] = [
  { value: 'off', label: 'Off' },
  { value: '1800:3', label: 'Every 30 minutes, 3 times' },
  { value: '3600:3', label: 'Every hour, 3 times' },
  { value: '3600:10', label: 'Every hour, 10 times' },
  { value: '86400:3', label: 'Every day, 3 times' },
];

export const MESSAGE_LIMIT_OPTIONS: { value: string; label: string }[] = [
  { value: 'off', label: 'No limit' },
  { value: '5', label: '5 messages an hour' },
  { value: '10', label: '10 messages an hour' },
  { value: '20', label: '20 messages an hour' },
  { value: '50', label: '50 messages an hour' },
];

export const WEEKDAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

type MuteMatch = Pick<Schemas['Alert'], 'rule_id' | 'server_id' | 'check_id' | 'cron_id' | 'subject'>;

export function getAlertMute(mutes: Schemas['AlertMute'][], alert: MuteMatch) {
  const targetId = getAlertTargetId(alert);
  return mutes.find(
    (mute) =>
      mute.rule_id === alert.rule_id &&
      (!mute.subject || mute.subject === alert.subject) &&
      (isWholeRuleMute(mute) || mute.server_id === targetId || mute.check_id === targetId || mute.cron_id === targetId)
  );
}

export function getMutedService(alert: Pick<Schemas['Alert'], 'server_id' | 'subject'>) {
  return alert.server_id === OTLP_SERVER_ID ? alert.subject : null;
}

export function isWholeRuleMute(mute: Schemas['AlertMute']) {
  return !mute.server_id && !mute.check_id && !mute.cron_id;
}

export function getAlertTargetId(alert: Pick<Schemas['Alert'], 'server_id' | 'check_id' | 'cron_id'>) {
  return alert.server_id ?? alert.check_id ?? alert.cron_id;
}

export function getRuleState(
  rule: Schemas['AlertRule'],
  firing: Schemas['Alert'][],
  mutes: Schemas['AlertMute'][],
  now = Date.now()
) {
  const own = firing.filter((alert) => alert.rule_id === rule.id);
  const isWholeMuted = mutes.some((mute) => mute.rule_id === rule.id && isWholeRuleMute(mute));
  if (!rule.enabled) {
    return 'disabled';
  }
  if (isWholeMuted || (own.length > 0 && own.every((alert) => getAlertMute(mutes, alert)))) {
    return 'muted';
  }
  if (rule.options.message_limit != null && rule.quiet_until && new Date(rule.quiet_until).getTime() > now) {
    return 'limited';
  }
  return own.length > 0 ? 'firing' : 'ok';
}

export function describeMuteEnd(mute: Schemas['AlertMute']) {
  if (!mute.ends_at) {
    return 'until it resolves';
  }
  const endsAt = new Date(mute.ends_at).getTime();
  const isToday = new Date(endsAt).toDateString() === new Date().toDateString();
  return isToday ? `until ${formatClock(endsAt)}` : `until ${formatDate(endsAt)}, ${formatClock(endsAt)}`;
}

export const SEVERITY_LABEL_OPTIONS: { value: string; label: string }[] = [
  { value: 'none', label: 'None' },
  { value: 'P1', label: 'P1' },
  { value: 'P2', label: 'P2' },
  { value: 'P3', label: 'P3' },
  { value: 'P4', label: 'P4' },
];

export type NotifyLevel = 'both' | 'critical' | 'off';

export const NOTIFY_OPTIONS: { value: NotifyLevel; label: string }[] = [
  { value: 'both', label: 'Critical and warning' },
  { value: 'critical', label: 'Critical only' },
  { value: 'off', label: 'Off' },
];

export const LOG_LEVEL_OPTIONS: { value: string; label: string }[] = [
  { value: 'error', label: 'Errors' },
  { value: 'warn', label: 'Warnings and errors' },
  { value: 'info', label: 'Info and above' },
];

export const LOG_WINDOW_OPTIONS: { value: number; label: string }[] = [
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
  { value: 3600, label: '1 hour' },
];

export type LogCondition = Extract<Schemas['Condition'], { kind: 'logs' }>;

export function readLogCondition(condition?: Schemas['Condition']) {
  return condition?.kind === 'logs' ? condition : undefined;
}

export const FOR_OPTIONS: { value: number; label: string }[] = [
  { value: 0, label: 'Immediately' },
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
  { value: 3600, label: '1 hour' },
];

export const INTEGRATION_KINDS: Record<
  IntegrationKind,
  { label: string; icon: IconName; description: string }
> = {
  slack: { label: 'Slack', icon: 'logo-slack', description: 'Post to a channel' },
  discord: { label: 'Discord', icon: 'logo-discord', description: 'Post to a channel' },
  telegram: { label: 'Telegram', icon: 'logo-telegram', description: 'Send to a chat with your bot' },
  webhook: { label: 'Webhook', icon: 'webhook', description: 'POST JSON to any URL' },
};

export function isIntegrationKind(value: string): value is IntegrationKind {
  return value in INTEGRATION_KINDS;
}

export function hasThreshold(kind: ConditionKind) {
  return hasAggregation(kind) || kind === 'cert_days' || kind === 'logs';
}

export function hasSubjectName(kind: ConditionKind) {
  return kind === 'process_stopped' || kind === 'port_closed';
}

export function readThreshold(condition: Schemas['Condition']) {
  if ('above' in condition) {
    return condition.above;
  }
  if ('below' in condition) {
    return condition.below;
  }
  return condition.kind === 'logs' ? condition.count : undefined;
}

export function readSubjectName(condition: Schemas['Condition']) {
  return 'name' in condition ? (condition.name ?? undefined) : undefined;
}

type Named = { id: string; name: string };

export function describeScope(
  scope: Scope,
  servers: Named[],
  checks: Named[] = [],
  isCheck = false,
  crons: Named[] = [],
  isCron = false
) {
  if (scope.kind === 'all') {
    return isCron ? 'All crons' : isCheck ? 'All checks' : 'All servers';
  }
  if (scope.kind === 'label') {
    return `label: ${scope.label}`;
  }
  const [ids, items, noun] =
    scope.kind === 'checks'
      ? [scope.check_ids, checks, 'checks']
      : scope.kind === 'crons'
        ? [scope.cron_ids, crons, 'crons']
        : [scope.server_ids, servers, 'servers'];
  const names = ids.map((id) => items.find((item) => item.id === id)?.name ?? id);
  return names.length > 2 ? `${names.length} ${noun}` : names.join(', ');
}

export function formatForSecs(secs: number) {
  return FOR_OPTIONS.find((option) => option.value === secs)?.label ?? `${Math.round(secs / 60)} minutes`;
}

export function formatAlertDuration(startedAt: string, endedAt?: string | null, now = Date.now()) {
  const end = endedAt ? new Date(endedAt).getTime() : now;
  const minutes = Math.max(Math.round((end - new Date(startedAt).getTime()) / 60_000), 0);
  if (minutes < 60) {
    return `${minutes}m`;
  }
  const hours = Math.floor(minutes / 60);
  return minutes % 60 ? `${hours}h ${minutes % 60}m` : `${hours}h`;
}

export function joinNames(names: string[]) {
  if (names.length <= 1) {
    return names[0] ?? '';
  }
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
}

export function getAlertTargetPath(alert: Pick<Schemas['Alert'], 'server_id' | 'check_id' | 'cron_id' | 'subject'>) {
  if (alert.cron_id) {
    return `/crons/${alert.cron_id}`;
  }
  if (alert.server_id === OTLP_SERVER_ID) {
    return `/logs?${new URLSearchParams({ server_id: OTLP_SERVER_ID, service: alert.subject })}`;
  }
  return alert.check_id ? `/uptime/${alert.check_id}` : `/servers/${alert.server_id}`;
}

export function formatLimit(kind: ConditionKind, value: number) {
  const unit = getUnit(kind);
  if (unit === '%') {
    return `${value}%`;
  }
  return unit ? `${value} ${unit}` : value.toString();
}

export function describeResolve(condition?: Schemas['Condition']) {
  if (!condition) {
    return 'Ends when the alert resolves';
  }
  const limit = readThreshold(condition);
  switch (condition.kind) {
    case 'offline':
      return 'Ends when the server reports again';
    case 'uptime_down':
      return 'Ends when the check is up again';
    case 'process_stopped':
      return `Ends when ${readSubjectName(condition) ?? 'the process'} runs again`;
    case 'port_closed':
      return `Ends when ${readSubjectName(condition) ?? 'the port'} opens again`;
    case 'cert_days':
      return 'Ends when the certificate is renewed';
    case 'logs':
      return `Ends when there are fewer than ${limit} matching lines`;
    default: {
      const names: Partial<Record<ConditionKind, string>> = {
        cpu: 'CPU',
        memory: 'memory',
        disk: 'disk',
        load: 'load',
        response_time: 'the response time',
        first_byte: 'the first byte time',
      };
      return `Ends when ${names[condition.kind] ?? 'it'} drops below ${formatLimit(condition.kind, limit ?? 0)}`;
    }
  }
}

export function describeAlertNow(alert: Schemas['Alert'], summary?: string) {
  const rest = summary?.match(/ (above|below|over) .*/)?.[0];
  return rest ? `${alert.subject} is at ${alert.value},${rest}.` : `${alert.subject}: ${alert.value}.`;
}

export function formatMinutesLong(from: string | number, to: string | number = Date.now()) {
  const minutes = Math.max(Math.round((new Date(to).getTime() - new Date(from).getTime()) / 60_000), 1);
  if (minutes < 60) {
    return `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return `${hours} ${hours === 1 ? 'hour' : 'hours'}`;
  }
  return `${Math.round(hours / 24)} days`;
}

export function describeFiring(alert: Schemas['Alert']) {
  const minutes = Math.max(Math.round((Date.now() - new Date(alert.started_at).getTime()) / 60_000), 1);
  const duration =
    minutes < 60 ? `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}` : `${formatAlertDuration(alert.started_at)}`;
  if (alert.subject === 'Server') {
    return `Offline for ${duration}`;
  }
  if (alert.value === 'Stopped' || alert.value === 'Closed') {
    return `${alert.subject} ${alert.value.toLowerCase()} for ${duration}`;
  }
  if (alert.subject === 'Status') {
    return `${alert.value} for ${duration}`;
  }
  return `${alert.subject} at ${alert.value} for ${duration}`;
}
