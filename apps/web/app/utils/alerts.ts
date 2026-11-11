import type { Schemas } from '@daroyan/client';

import type { IconName } from '~/components/icons/icon-paths';
import { formatClock, formatDate, formatUnit } from '~/utils/format';
import { OTLP_SERVER_ID } from '~/utils/logs';
import { METRICS_TARGET_ID } from '~/utils/metrics';
import { getTraceService, TRACES_TARGET_ID } from '~/utils/traces';

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
      { kind: 'process_degraded', label: 'Process degraded' },
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
  {
    label: 'Traces',
    items: [
      { kind: 'trace_errors', label: 'Error rate' },
      { kind: 'trace_latency', label: 'Latency' },
    ],
  },
  { label: 'Metrics', items: [{ kind: 'metric', label: 'Metric value' }] },
];

export const CONDITION_OPTIONS = METRIC_GROUPS.flatMap((group) => group.items);

const CHECK_KINDS: ConditionKind[] = ['uptime_down', 'response_time', 'first_byte', 'cert_days'];
const CRON_KINDS: ConditionKind[] = ['cron_missed', 'cron_failed'];
const TRACE_KINDS: ConditionKind[] = ['trace_errors', 'trace_latency'];
const AGGREGATED_KINDS: ConditionKind[] = ['cpu', 'memory', 'disk', 'load', 'response_time', 'first_byte'];

export function isCheckKind(kind: ConditionKind) {
  return CHECK_KINDS.includes(kind);
}

export function isCronKind(kind: ConditionKind) {
  return CRON_KINDS.includes(kind);
}

export function isTraceKind(kind: ConditionKind) {
  return TRACE_KINDS.includes(kind);
}

export function isServiceKind(kind: ConditionKind) {
  return isTraceKind(kind) || kind === 'metric';
}

export function hasAggregation(kind: ConditionKind) {
  return AGGREGATED_KINDS.includes(kind);
}

export function getUnit(kind: ConditionKind) {
  switch (kind) {
    case 'cpu':
    case 'memory':
    case 'disk':
    case 'trace_errors':
      return '%';
    case 'response_time':
    case 'first_byte':
    case 'trace_latency':
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

export function getMutedService(alert: Pick<Schemas['Alert'], 'server_id' | 'subject'>, rule?: Schemas['AlertRule']) {
  if (alert.server_id === OTLP_SERVER_ID || alert.server_id === TRACES_TARGET_ID || alert.server_id === METRICS_TARGET_ID) {
    return alert.subject;
  }
  if (readLogCondition(rule?.condition)?.group_by === 'source') {
    return alert.subject;
  }
  return null;
}

export function isWholeRuleMute(mute: Schemas['AlertMute']) {
  return !mute.server_id && !mute.check_id && !mute.cron_id;
}

export type AlertTarget = {
  serverId?: string;
  checkId?: string;
  cronId?: string;
};

export function getAlertsForTarget(alerts: Schemas['Alert'][], target: AlertTarget) {
  const { serverId, checkId, cronId } = target;

  return alerts.filter(
    (alert) =>
      (serverId && alert.server_id === serverId) ||
      (checkId && alert.check_id === checkId) ||
      (cronId && alert.cron_id === cronId)
  );
}

export function getSharedMute(alerts: MuteMatch[], mutes: Schemas['AlertMute'][]) {
  const alertMutes = alerts.map((alert) => getAlertMute(mutes, alert));
  if (alertMutes.length === 0 || !alertMutes.every(Boolean)) {
    return undefined;
  }

  return alertMutes[0];
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
  if (isWholeMuted || getSharedMute(own, mutes)) {
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

export const TRACE_WINDOW_OPTIONS = LOG_WINDOW_OPTIONS;

export const PERCENTILE_OPTIONS: { value: Schemas['Percentile']; label: string }[] = [
  { value: 'p50', label: 'p50' },
  { value: 'p95', label: 'p95' },
  { value: 'p99', label: 'p99' },
];

export const TRACE_SPANS_OPTIONS: { value: Schemas['TraceSpans']; label: string }[] = [
  { value: 'entry', label: 'Incoming requests' },
  { value: 'all', label: 'All spans' },
];

export const TRACE_GROUP_OPTIONS: { value: Schemas['TraceGroup']; label: string }[] = [
  { value: 'service', label: 'Each service' },
  { value: 'route', label: 'Each route' },
];

export type TraceCondition = Extract<Schemas['Condition'], { kind: 'trace_errors' | 'trace_latency' }>;

export function readTraceCondition(condition?: Schemas['Condition']): TraceCondition | undefined {
  if (condition?.kind === 'trace_errors' || condition?.kind === 'trace_latency') {
    return condition;
  }
  return undefined;
}

export type MetricCondition = Extract<Schemas['Condition'], { kind: 'metric' }>;

export function readMetricCondition(condition?: Schemas['Condition']) {
  return condition?.kind === 'metric' ? condition : undefined;
}

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
  return hasAggregation(kind) || isServiceKind(kind) || kind === 'cert_days' || kind === 'logs';
}

export function isProcessKind(kind: ConditionKind) {
  return kind === 'process_stopped' || kind === 'process_degraded';
}

export function hasSubjectName(kind: ConditionKind) {
  return isProcessKind(kind) || kind === 'port_closed';
}

export function readThreshold(condition: Schemas['Condition']) {
  if (condition.kind === 'metric') {
    return condition.threshold;
  }
  if ('above' in condition) {
    return condition.above;
  }
  if ('below' in condition) {
    return condition.below;
  }
  return condition.kind === 'logs' ? condition.count : undefined;
}

export function readSubjectName(condition: Schemas['Condition']) {
  if (condition.kind === 'metric') {
    return undefined;
  }
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

export function getTraceScopeLabel(condition?: TraceCondition) {
  return condition?.service ?? 'All services';
}

export function getAlertNoun(alert: Pick<Schemas['Alert'], 'server_id' | 'check_id' | 'cron_id'>) {
  if (alert.check_id) {
    return 'check';
  }
  if (alert.cron_id) {
    return 'cron';
  }
  return alert.server_id === TRACES_TARGET_ID || alert.server_id === METRICS_TARGET_ID ? 'service' : 'server';
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

export function getAlertTargetPath(alert: Pick<Schemas['Alert'], 'server_id' | 'check_id' | 'cron_id' | 'subject'>) {
  if (alert.cron_id) {
    return `/crons/${alert.cron_id}`;
  }
  if (alert.server_id === OTLP_SERVER_ID) {
    return `/logs?${new URLSearchParams({ server_id: OTLP_SERVER_ID, service: alert.subject })}`;
  }
  if (alert.server_id === TRACES_TARGET_ID) {
    return `/traces?${new URLSearchParams({ service: getTraceService(alert.subject) })}`;
  }
  if (alert.server_id === METRICS_TARGET_ID) {
    return '/metrics';
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
    case 'process_degraded':
      return `Ends when ${readSubjectName(condition) ?? 'the process'} runs min_running containers again`;
    case 'port_closed':
      return `Ends when ${readSubjectName(condition) ?? 'the port'} opens again`;
    case 'cert_days':
      return 'Ends when the certificate is renewed';
    case 'logs':
      return `Ends when there are fewer than ${limit} matching lines`;
    case 'metric':
      return `Ends when ${condition.name} is back ${condition.below ? 'above' : 'below'} ${limit}`;
    default: {
      const names: Partial<Record<ConditionKind, string>> = {
        cpu: 'CPU',
        memory: 'memory',
        disk: 'disk',
        load: 'load',
        response_time: 'the response time',
        first_byte: 'the first byte time',
        trace_errors: 'the error rate',
        trace_latency: 'the latency',
      };
      return `Ends when ${names[condition.kind] ?? 'it'} drops below ${formatLimit(condition.kind, limit ?? 0)}`;
    }
  }
}

export function getValueHint(condition?: Schemas['Condition']) {
  if (!condition) {
    return undefined;
  }

  switch (condition.kind) {
    case 'cpu':
      return 'CPU usage when the alert fired.';
    case 'memory':
      return 'Memory usage when the alert fired.';
    case 'disk':
      return 'Disk usage when the alert fired.';
    case 'load':
      return 'Load average over 1 minute when the alert fired.';
    case 'response_time':
      return 'How long the check took to respond when the alert fired.';
    case 'first_byte':
      return 'How long the check took to send the first byte when the alert fired.';
    case 'cert_days':
      return 'Days left on the certificate when the alert fired.';
    case 'logs':
      return `Matching log lines in the ${formatUnit(condition.within_secs / 60, 'minute')} before the alert fired.`;
    case 'trace_errors':
      return `Share of requests that failed in the ${formatUnit(condition.within_secs / 60, 'minute')} before the alert fired.`;
    case 'metric':
      return `${condition.name} in the ${formatUnit(condition.within_secs / 60, 'minute')} before the alert fired.`;
    case 'trace_latency': {
      const percentile = condition.percentile ?? 'p95';
      return `${percentile} latency in the ${formatUnit(condition.within_secs / 60, 'minute')} before the alert fired. ${percentile} means ${percentile.slice(1)}% of requests finished faster than this.`;
    }
    default:
      return undefined;
  }
}

export function describeAlertNow(alert: Schemas['Alert'], summary?: string) {
  const rest = summary?.match(/ (above|below|over) .*/)?.[0];
  return rest ? `${alert.subject} is at ${alert.value},${rest}.` : `${alert.subject}: ${alert.value}.`;
}

export function formatMinutesLong(from: string | number, to: string | number = Date.now()) {
  const minutes = Math.max(Math.round((new Date(to).getTime() - new Date(from).getTime()) / 60_000), 1);
  if (minutes < 60) {
    return formatUnit(minutes, 'minute');
  }
  const hours = Math.round(minutes / 60);
  if (hours < 48) {
    return formatUnit(hours, 'hour');
  }
  return formatUnit(Math.round(hours / 24), 'day');
}

export function describeFiring(alert: Schemas['Alert']) {
  const minutes = Math.max(Math.round((Date.now() - new Date(alert.started_at).getTime()) / 60_000), 1);
  const duration =
    minutes < 60 ? formatUnit(minutes, 'minute') : formatAlertDuration(alert.started_at);
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
