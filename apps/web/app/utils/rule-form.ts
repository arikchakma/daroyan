import type { Schemas } from '@daroyan/client';

import type { Aggregation, ConditionKind, NoData, NotifyLevel, Scope } from '~/utils/alerts';
import {
  AGGREGATION_OPTIONS,
  getUnit,
  hasAggregation,
  hasThreshold,
  isCheckKind,
  isCronKind,
  joinNames,
  readLogCondition,
  readSubjectName,
  readThreshold,
  REMIND_OPTIONS,
  RESOLVE_OPTIONS,
  WINDOW_OPTIONS,
} from '~/utils/alerts';

export type ScopeKind = 'all' | 'label' | 'picked';

export type TargetNoun = 'server' | 'check' | 'cron';

export type SelectItemData = { value: string; label: string };

export type RuleFormState = {
  name: string;
  kind: ConditionKind;
  critical: string;
  warning: string;
  isWarningOpen: boolean;
  recovery: string;
  aggregation: Aggregation;
  windowSecs: number;
  forSecs: number;
  resolveForSecs: number;
  noData: NoData;
  remind: string;
  messageLimit: string;
  quietHours: Schemas['QuietWindow'][];
  timeZone: string;
  subjectName: string;
  logLevel: string;
  logWithin: number;
  logService: string;
  logText: string;
  scopeKind: ScopeKind;
  label: string;
  pickedIds: string[];
  declareIncident: boolean;
  declareSeverity: string;
  declareShown: string;
  runbookUrl: string;
};

export type RuleFormChange = (patch: Partial<RuleFormState>) => void;

export type RuleDraft = {
  name: string;
  condition: Schemas['Condition'];
  scope?: Scope;
  forSecs?: number;
};

export type RuleTemplate = {
  label: string;
  name: string;
  kind: ConditionKind;
  critical?: string;
  warning?: string;
  aggregation?: Aggregation;
  windowSecs?: number;
  forSecs: number;
};

export const TEMPLATES: RuleTemplate[] = [
  { label: 'High CPU', name: 'High CPU', kind: 'cpu', critical: '90', warning: '75', windowSecs: 300, forSecs: 0 },
  { label: 'Server offline', name: 'Server Offline', kind: 'offline', forSecs: 60 },
  { label: 'Disk almost full', name: 'Disk Almost Full', kind: 'disk', critical: '90', warning: '80', forSecs: 900 },
  { label: 'Memory pressure', name: 'Memory Pressure', kind: 'memory', critical: '90', windowSecs: 300, forSecs: 0 },
  { label: 'Process stopped', name: 'Process Stopped', kind: 'process_stopped', forSecs: 60 },
  { label: 'Errors in logs', name: 'Errors in Logs', kind: 'logs', critical: '10', forSecs: 0 },
  { label: 'Uptime check down', name: 'Uptime Check Down', kind: 'uptime_down', forSecs: 0 },
  {
    label: 'Slow responses',
    name: 'Slow Responses',
    kind: 'response_time',
    critical: '1000',
    warning: '500',
    aggregation: 'p95',
    windowSecs: 300,
    forSecs: 60,
  },
  { label: 'Certificate expiring', name: 'Certificate Expiring', kind: 'cert_days', critical: '7', warning: '14', forSecs: 0 },
];

const DEFAULT_THRESHOLDS: Partial<Record<ConditionKind, string>> = {
  cpu: '90',
  memory: '90',
  disk: '85',
  load: '4',
  response_time: '1000',
  first_byte: '800',
  cert_days: '7',
  logs: '10',
};

export const CONDITION_LABELS: Record<ConditionKind, string> = {
  cpu: 'CPU above a limit',
  memory: 'Memory above a limit',
  disk: 'Disk above a limit',
  load: 'Load above a limit',
  offline: 'Server offline',
  process_stopped: 'Process stopped',
  port_closed: 'Port closed',
  uptime_down: 'Check is down',
  response_time: 'Response time above a limit',
  first_byte: 'First byte above a limit',
  cert_days: 'Certificate expires soon',
  cron_missed: 'Cron missed a run',
  cron_failed: 'Cron failed',
  logs: 'Log lines match',
};

export const DECLARE_SEVERITY_OPTIONS = [
  { value: 'minor', label: 'Minor' },
  { value: 'major', label: 'Major' },
  { value: 'critical', label: 'Critical' },
];

export const DECLARE_SHOWN_OPTIONS = [
  { value: 'shown', label: 'Yes, as an affected component' },
  { value: 'hidden', label: 'No, internal until someone posts' },
];

export const DEFAULT_MORE_OPTIONS: Partial<RuleFormState> = {
  recovery: '',
  resolveForSecs: 0,
  noData: 'keep',
  remind: 'off',
  messageLimit: 'off',
  quietHours: [],
  declareIncident: false,
  runbookUrl: '',
};

export function readNumber(value: string) {
  return value.trim() === '' ? null : Number(value);
}

export function isOptionalNumber(value: string) {
  const number = readNumber(value);
  return number === null || Number.isFinite(number);
}

export function readLimit(form: RuleFormState) {
  const value = readNumber(form.critical);
  return value !== null && Number.isFinite(value) && value > 0 ? value : null;
}

function readRecoveryLimit(form: RuleFormState) {
  const warning = readNumber(form.warning);
  if (hasWarning(form.kind) && form.isWarningOpen && warning !== null && Number.isFinite(warning) && warning > 0) {
    return warning;
  }
  return readLimit(form);
}

export function readRecovery(form: RuleFormState) {
  const recovery = readNumber(form.recovery);
  const limit = readRecoveryLimit(form);

  if (!hasThreshold(form.kind) || recovery === null || !Number.isFinite(recovery) || limit === null) {
    return null;
  }

  const isSafe = form.kind === 'cert_days' ? recovery >= limit : recovery <= limit;
  return isSafe ? recovery : null;
}

export function isLimitValid(form: RuleFormState) {
  return !hasThreshold(form.kind) || readLimit(form) !== null;
}

function readScopeKind(scope?: Scope): ScopeKind {
  if (scope?.kind === 'label') {
    return 'label';
  }
  if (scope?.kind === 'servers' || scope?.kind === 'checks' || scope?.kind === 'crons') {
    return 'picked';
  }
  return 'all';
}

function readPickedIds(scope?: Scope) {
  if (scope?.kind === 'servers') {
    return scope.server_ids;
  }
  if (scope?.kind === 'checks') {
    return scope.check_ids;
  }
  return scope?.kind === 'crons' ? scope.cron_ids : [];
}

export function readTargetNoun(kind: ConditionKind): TargetNoun {
  if (isCheckKind(kind)) {
    return 'check';
  }
  return isCronKind(kind) ? 'cron' : 'server';
}

export function readTargetNouns(noun: TargetNoun) {
  return `${noun[0]!.toUpperCase()}${noun.slice(1)}s`;
}

export function hasWarning(kind: ConditionKind) {
  return hasThreshold(kind) && kind !== 'logs';
}

export function formatAmount(value: number, unit: string) {
  if (unit === '%') {
    return `${value}%`;
  }
  return unit ? `${value} ${unit}` : `${value}`;
}

export function formatSpan(seconds: number) {
  return WINDOW_OPTIONS.find((option) => option.value === seconds)?.label ?? `${Math.round(seconds / 60)} minutes`;
}

export function getMatchesText(names: string[], noun: string) {
  if (names.length === 0) {
    return `Matches no ${noun}s yet.`;
  }

  const count = names.length === 1 ? `1 ${noun}` : `${names.length} ${noun}s`;
  return `Matches ${count}: ${joinNames(names)}.`;
}

type ReadInitialRuleFormOptions = {
  rule?: Schemas['AlertRule'];
  draft?: RuleDraft;
  timeZone: string;
};

export function readInitialRuleForm(options: ReadInitialRuleFormOptions): RuleFormState {
  const { rule, draft, timeZone } = options;

  const saved = rule?.options;
  const initial = rule?.condition ?? draft?.condition;
  const savedLog = readLogCondition(initial);
  const scope = rule?.scope ?? draft?.scope;

  return {
    name: rule?.name ?? draft?.name ?? '',
    kind: initial?.kind ?? 'cpu',
    critical: initial ? (readThreshold(initial)?.toString() ?? '') : (DEFAULT_THRESHOLDS.cpu ?? ''),
    warning: saved?.warning?.toString() ?? '',
    isWarningOpen: saved?.warning != null,
    recovery: saved?.resolve_at?.toString() ?? '',
    aggregation: saved?.aggregation ?? 'avg',
    windowSecs: saved?.window_secs ?? (rule ? 0 : 300),
    forSecs: rule?.for_secs ?? draft?.forSecs ?? 300,
    resolveForSecs: saved?.resolve_for_secs ?? 0,
    noData: saved?.no_data ?? 'keep',
    remind: saved?.remind_every_secs ? `${saved.remind_every_secs}:${saved.remind_times ?? 1}` : 'off',
    messageLimit: saved?.message_limit?.toString() ?? 'off',
    quietHours: saved?.quiet_hours ?? [],
    timeZone: saved?.time_zone ?? timeZone,
    subjectName: initial ? (readSubjectName(initial) ?? '') : '',
    logLevel: savedLog?.level ?? 'error',
    logWithin: savedLog?.within_secs ?? 300,
    logService: savedLog?.service ?? '',
    logText: savedLog?.text ?? '',
    scopeKind: readScopeKind(scope),
    label: scope?.kind === 'label' ? scope.label : '',
    pickedIds: readPickedIds(scope),
    declareIncident: rule?.declare_incident ?? false,
    declareSeverity: rule?.declare_severity ?? 'major',
    declareShown: rule && rule.declare_hidden_page_ids.length > 0 ? 'hidden' : 'shown',
    runbookUrl: saved?.runbook_url ?? '',
  };
}

export function readInitialNotifyLevels(rule?: Schemas['AlertRule']) {
  if (!rule) {
    return undefined;
  }

  return Object.fromEntries(
    rule.integration_ids.map((id): [string, NotifyLevel] => [
      id,
      rule.options.warning_integration_ids?.includes(id) ? 'both' : 'critical',
    ])
  );
}

export function changeKind(form: RuleFormState, next: ConditionKind): Partial<RuleFormState> {
  const isSameNoun = readTargetNoun(next) === readTargetNoun(form.kind);

  return {
    ...(isSameNoun ? {} : { scopeKind: 'all', pickedIds: [] }),
    kind: next,
    critical: DEFAULT_THRESHOLDS[next] ?? '',
    warning: '',
    isWarningOpen: false,
    recovery: '',
    aggregation: next === 'response_time' || next === 'first_byte' ? 'p95' : 'avg',
  };
}

export function applyTemplate(form: RuleFormState, template: RuleTemplate): Partial<RuleFormState> {
  return {
    ...changeKind(form, template.kind),
    name: template.name,
    critical: template.critical ?? '',
    warning: template.warning ?? '',
    isWarningOpen: !!template.warning,
    aggregation: template.aggregation ?? 'avg',
    windowSecs: template.windowSecs ?? 0,
    subjectName: '',
    forSecs: template.forSecs,
  };
}

export function readCondition(form: RuleFormState): Schemas['Condition'] | undefined {
  const { kind } = form;

  if (!isLimitValid(form)) {
    return undefined;
  }

  const limit = readLimit(form) ?? 0;

  switch (kind) {
    case 'offline':
    case 'uptime_down':
    case 'cron_missed':
    case 'cron_failed':
      return { kind };
    case 'process_stopped':
    case 'port_closed':
      return { kind, name: form.subjectName.trim() || null };
    case 'logs':
      return Number.isInteger(limit)
        ? {
            kind,
            level: form.logLevel,
            count: limit,
            within_secs: form.logWithin,
            service: form.logService || null,
            text: form.logText.trim() || null,
          }
        : undefined;
    case 'cert_days':
      return { kind, below: limit };
    case 'cpu':
    case 'memory':
    case 'disk':
    case 'load':
    case 'response_time':
    case 'first_byte':
      return { kind, above: limit };
  }
}

export function readScope(form: RuleFormState): Scope | undefined {
  const noun = readTargetNoun(form.kind);

  if (form.scopeKind === 'label' && noun === 'server') {
    return form.label ? { kind: 'label', label: form.label } : undefined;
  }
  if (form.scopeKind !== 'picked') {
    return { kind: 'all' };
  }
  if (form.pickedIds.length === 0) {
    return undefined;
  }

  if (noun === 'check') {
    return { kind: 'checks', check_ids: form.pickedIds };
  }
  return noun === 'cron' ? { kind: 'crons', cron_ids: form.pickedIds } : { kind: 'servers', server_ids: form.pickedIds };
}

type ReadRuleOptionsOptions = {
  form: RuleFormState;
  warningIntegrationIds: string[];
  severityLabel: string | null;
};

export function readRuleOptions(options: ReadRuleOptionsOptions): Schemas['RuleOptions'] {
  const { form, warningIntegrationIds, severityLabel } = options;

  const [remindEvery, remindTimes] = form.remind === 'off' ? [null, 0] : form.remind.split(':').map(Number);

  return {
    aggregation: form.aggregation,
    window_secs: hasAggregation(form.kind) ? form.windowSecs : 0,
    warning: hasWarning(form.kind) ? readNumber(form.warning) : null,
    resolve_at: readRecovery(form),
    resolve_for_secs: form.resolveForSecs,
    no_data: readTargetNoun(form.kind) === 'cron' ? 'keep' : form.noData,
    remind_every_secs: remindEvery,
    remind_times: remindTimes ?? 0,
    runbook_url: form.runbookUrl.trim() || null,
    severity_label: severityLabel,
    warning_integration_ids: warningIntegrationIds,
    message_limit: form.messageLimit === 'off' ? null : Number(form.messageLimit),
    quiet_hours: form.quietHours,
    time_zone: form.timeZone,
  };
}

export function getMeasureGroups(windowSecs: number) {
  const windows = [...new Set([...WINDOW_OPTIONS.map((option) => option.value), windowSecs])]
    .filter((secs) => secs > 0)
    .sort((a, b) => a - b);

  return [
    [{ value: 'latest', label: 'Latest reading' }],
    ...AGGREGATION_OPTIONS.map((option) =>
      windows.map((secs) => ({ value: `${option.value}:${secs}`, label: `${option.label} over ${formatSpan(secs)}` }))
    ),
  ];
}

function getRecoveryPresets(kind: ConditionKind, limit: number | null) {
  if (limit === null) {
    return [];
  }

  if (kind === 'cert_days') {
    return [limit + 7, limit + 14];
  }
  if (getUnit(kind) === '%') {
    return [limit - 5, limit - 10].filter((value) => value > 0);
  }
  return [Math.round(limit * 0.8), Math.round(limit * 0.5)].filter((value) => value > 0 && value !== limit);
}

function getRecoveryLabel(form: RuleFormState, value: number | null) {
  const verb = form.kind === 'cert_days' ? 'Over' : 'Under';
  const amount = value ?? readRecoveryLimit(form);

  return `${verb} ${amount === null ? 'the limit' : formatAmount(amount, getUnit(form.kind))}`;
}

function getRecoveredLabel(kind: ConditionKind, secs: number) {
  if (secs > 0) {
    return `${formatSpan(secs)} after it recovers`;
  }
  return kind === 'offline' ? 'As soon as data comes back' : 'As soon as it recovers';
}

export function getResolveGroups(form: RuleFormState) {
  if (!hasThreshold(form.kind)) {
    return [RESOLVE_OPTIONS.map((option) => ({ value: `limit:${option.value}`, label: getRecoveredLabel(form.kind, option.value) }))];
  }

  const recovery = readRecovery(form);
  const choices = [
    ...new Set([...getRecoveryPresets(form.kind, readRecoveryLimit(form)), ...(recovery !== null ? [recovery] : [])]),
  ];

  return [null, ...choices].map((value) =>
    RESOLVE_OPTIONS.map((option) => {
      const label = getRecoveryLabel(form, value);
      return {
        value: `${value ?? 'limit'}:${option.value}`,
        label: option.value === 0 ? label : `${label} for ${formatSpan(option.value)}`,
      };
    })
  );
}

export type ChangedOptions = ReturnType<typeof getChangedOptions>;

export function getChangedOptions(form: RuleFormState) {
  return {
    resolve: readRecovery(form) !== null || form.resolveForSecs !== 0,
    noData: readTargetNoun(form.kind) !== 'cron' && form.noData !== 'keep',
    remind: form.remind !== 'off',
    messageLimit: form.messageLimit !== 'off',
    quietHours: form.quietHours.length > 0,
    incident: form.declareIncident,
    runbook: form.runbookUrl.trim() !== '',
  };
}

function getResolveSummary(form: RuleFormState) {
  const recovery = readRecovery(form);

  if (recovery !== null) {
    const span = form.resolveForSecs > 0 ? ` for ${formatSpan(form.resolveForSecs)}` : '';
    return `Resolves ${getRecoveryLabel(form, recovery).toLowerCase()}${span}`;
  }

  return `Resolves ${getRecoveredLabel(form.kind, form.resolveForSecs).toLowerCase()}`;
}

export function getMoreOptionsSummary(form: RuleFormState) {
  const changed = getChangedOptions(form);
  const remind = REMIND_OPTIONS.find((option) => option.value === form.remind)?.label ?? 'while firing';

  return [
    getResolveSummary(form),
    changed.noData && (form.noData === 'alert' ? 'Alerts when data stops' : 'Resolves when data stops'),
    form.remind === 'off' ? 'No reminders' : `Reminds ${remind.toLowerCase()}`,
    changed.messageLimit && `At most ${form.messageLimit} messages an hour`,
    changed.quietHours ? 'Quiet hours on' : 'No quiet hours',
    form.declareIncident ? 'Opens an incident' : 'No incident',
    changed.runbook && 'Runbook linked',
  ]
    .filter(Boolean)
    .join(' · ');
}

export function getChangedCountText(count: number) {
  if (count === 1) {
    return '1 setting differs from the defaults and is marked. Reset puts it back.';
  }
  return `${count} settings differ from the defaults and are marked. Reset puts them back.`;
}
