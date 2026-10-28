import type { Schemas } from '@daroyan/client';
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query';
import { Fragment, useId, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { Link, useNavigate } from 'react-router';

import { IntegrationDialog } from '~/components/alerts/integration-dialog';
import { getIntegrationLabel } from '~/components/alerts/integration-logo';
import { DEFAULT_QUIET_WINDOW, getBrowserTimeZone, QuietHoursField } from '~/components/alerts/quiet-hours-field';
import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Checkbox } from '~/components/ui/checkbox';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { Select } from '~/components/ui/select';
import { Switch } from '~/components/ui/switch';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  backtestAlertRuleOptions,
  createAlertRuleOptions,
  deleteAlertRuleOptions,
  listAlertsOptions,
  listIntegrationsOptions,
  testAlertRuleOptions,
  updateAlertRuleOptions,
} from '~/queries/alerts';
import { logSourcesOptions } from '~/queries/logs';
import { listServersOptions } from '~/queries/servers';
import { listStatusPagesOptions } from '~/queries/status-pages';
import { listUptimeChecksOptions } from '~/queries/uptime';
import type { Aggregation, ConditionKind, NoData, NotifyLevel, Scope } from '~/utils/alerts';
import {
  AGGREGATION_OPTIONS,
  FOR_OPTIONS,
  getUnit,
  hasAggregation,
  hasSubjectName,
  hasThreshold,
  INTEGRATION_KINDS,
  isCheckKind,
  isCronKind,
  isIntegrationKind,
  joinNames,
  LOG_LEVEL_OPTIONS,
  LOG_WINDOW_OPTIONS,
  MESSAGE_LIMIT_OPTIONS,
  METRIC_GROUPS,
  NO_DATA_OPTIONS,
  NOTIFY_OPTIONS,
  readLogCondition,
  readSubjectName,
  readThreshold,
  REMIND_OPTIONS,
  RESOLVE_OPTIONS,
  WINDOW_OPTIONS,
} from '~/utils/alerts';
import { cn } from '~/utils/classname';
import { formatDate } from '~/utils/format';

type ScopeKind = 'all' | 'label' | 'picked';

type RuleTemplate = {
  label: string;
  name: string;
  kind: ConditionKind;
  critical?: string;
  warning?: string;
  aggregation?: Aggregation;
  windowSecs?: number;
  forSecs: number;
};

const TEMPLATES: RuleTemplate[] = [
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

const RECENT_MS = 30 * 86_400_000;

const CONDITION_LABELS: Record<ConditionKind, string> = {
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

export type RuleDraft = {
  name: string;
  condition: Schemas['Condition'];
  scope?: Scope;
  forSecs?: number;
};

type RuleFormProps = {
  rule?: Schemas['AlertRule'];
  draft?: RuleDraft;
};

export function RuleForm(props: RuleFormProps) {
  const { rule, draft } = props;

  const [version, setVersion] = useState(0);

  return <RuleFormBody key={version} rule={rule} draft={draft} onDiscard={() => setVersion(version + 1)} />;
}

type RuleFormBodyProps = RuleFormProps & {
  onDiscard: () => void;
};

function readNumber(value: string) {
  return value.trim() === '' ? null : Number(value);
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

function readTargetNoun(kind: ConditionKind) {
  if (isCheckKind(kind)) {
    return 'check';
  }
  return isCronKind(kind) ? 'cron' : 'server';
}

function hasWarning(kind: ConditionKind) {
  return hasThreshold(kind) && kind !== 'logs';
}

function formatAmount(value: number, unit: string) {
  if (unit === '%') {
    return `${value}%`;
  }
  return unit ? `${value} ${unit}` : `${value}`;
}

function formatSpan(seconds: number) {
  return WINDOW_OPTIONS.find((option) => option.value === seconds)?.label ?? `${Math.round(seconds / 60)} minutes`;
}

function getRecoveryPresets(kind: ConditionKind, limit: number | null) {
  if (limit === null || !Number.isFinite(limit) || limit <= 0) {
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

function RuleFormBody(props: RuleFormBodyProps) {
  const { rule, draft, onDiscard } = props;

  const formRef = useRef<HTMLFormElement>(null);
  const navigate = useNavigate();
  const isAdmin = useIsAdmin();
  const { data: servers } = useQuery(listServersOptions());
  const { data: checks } = useQuery(listUptimeChecksOptions());
  const { data: crons } = useQuery({ queryKey: ['crons'], queryFn: () => request(apiClient.GET('/v1/crons')) });
  const { data: integrations } = useQuery(listIntegrationsOptions());
  const { data: logSources } = useQuery(logSourcesOptions());
  const { data: history = [] } = useQuery({ ...listAlertsOptions('all', rule?.id), enabled: !!rule });
  const saved = rule?.options;
  const initial = rule?.condition ?? draft?.condition;

  const [name, setName] = useState(rule?.name ?? draft?.name ?? '');
  const [runbookUrl, setRunbookUrl] = useState(saved?.runbook_url ?? '');
  const [kind, setKind] = useState<ConditionKind>(initial?.kind ?? 'cpu');
  const [critical, setCritical] = useState(
    initial ? (readThreshold(initial)?.toString() ?? '') : (DEFAULT_THRESHOLDS.cpu ?? '')
  );
  const [warning, setWarning] = useState(saved?.warning?.toString() ?? '');
  const [isWarningOpen, setIsWarningOpen] = useState(saved?.warning != null);
  const [recovery, setRecovery] = useState(saved?.resolve_at?.toString() ?? '');
  const [aggregation, setAggregation] = useState<Aggregation>(saved?.aggregation ?? 'avg');
  const [windowSecs, setWindowSecs] = useState(saved?.window_secs ?? (rule ? 0 : 300));
  const [forSecs, setForSecs] = useState(rule?.for_secs ?? draft?.forSecs ?? 300);
  const [resolveForSecs, setResolveForSecs] = useState(saved?.resolve_for_secs ?? 0);
  const [noData, setNoData] = useState<NoData>(saved?.no_data ?? 'keep');
  const [remind, setRemind] = useState(
    saved?.remind_every_secs ? `${saved.remind_every_secs}:${saved.remind_times ?? 1}` : 'off'
  );
  const [messageLimit, setMessageLimit] = useState(saved?.message_limit?.toString() ?? 'off');
  const [quietHours, setQuietHours] = useState(saved?.quiet_hours ?? []);
  const [timeZone, setTimeZone] = useState(saved?.time_zone ?? getBrowserTimeZone());
  const [subjectName, setSubjectName] = useState(initial ? (readSubjectName(initial) ?? '') : '');
  const savedLog = readLogCondition(initial);
  const [logLevel, setLogLevel] = useState(savedLog?.level ?? 'error');
  const [logWithin, setLogWithin] = useState(savedLog?.within_secs ?? 300);
  const [logService, setLogService] = useState(savedLog?.service ?? '');
  const [logText, setLogText] = useState(savedLog?.text ?? '');
  const initialScope = rule?.scope ?? draft?.scope;
  const [scopeKind, setScopeKind] = useState<ScopeKind>(readScopeKind(initialScope));
  const [label, setLabel] = useState(initialScope?.kind === 'label' ? initialScope.label : '');
  const [pickedIds, setPickedIds] = useState<string[]>(
    initialScope?.kind === 'servers'
      ? initialScope.server_ids
      : initialScope?.kind === 'checks'
        ? initialScope.check_ids
        : initialScope?.kind === 'crons'
          ? initialScope.cron_ids
          : []
  );
  const [notifyLevels, setNotifyLevels] = useState<Record<string, NotifyLevel> | undefined>(() =>
    rule
      ? Object.fromEntries(
          rule.integration_ids.map((id) => [id, saved?.warning_integration_ids?.includes(id) ? 'both' : 'critical'])
        )
      : undefined
  );
  const [shownIntegrationIds, setShownIntegrationIds] = useState<string[]>([]);
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [declareIncident, setDeclareIncident] = useState(rule?.declare_incident ?? false);
  const [declareSeverity, setDeclareSeverity] = useState(rule?.declare_severity ?? 'major');
  const [declareShown, setDeclareShown] = useState(
    rule && rule.declare_hidden_page_ids.length > 0 ? 'hidden' : 'shown'
  );
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const { data: statusPages = [] } = useQuery(listStatusPagesOptions());

  const noun = readTargetNoun(kind);
  const unit = getUnit(kind);
  const levels: Record<string, NotifyLevel> =
    notifyLevels ?? Object.fromEntries((integrations ?? []).map((integration) => [integration.id, 'both']));
  const integrationIds = Object.keys(levels).filter((id) => levels[id] !== 'off');
  const warningIntegrationIds = Object.keys(levels).filter((id) => levels[id] === 'both');
  const labels = [...new Set((servers ?? []).flatMap((server) => server.labels))].sort();
  const logServices = [...new Set((logSources ?? []).map((source) => source.service))].sort();
  const targets: { id: string; name: string; detail: string; labels: string[] }[] =
    noun === 'check'
      ? (checks ?? []).map((check) => ({ id: check.id, name: check.name, detail: check.target, labels: [] }))
      : noun === 'cron'
        ? (crons ?? []).map((cron) => ({ id: cron.id, name: cron.name, detail: cron.expression ?? '', labels: [] }))
        : (servers ?? []).map((server) => ({ id: server.id, name: server.name, detail: server.labels.join(', '), labels: server.labels }));

  const criticalValue = readNumber(critical);
  const warningValue = readNumber(warning);
  const recoveryValue = readNumber(recovery);
  const isValid = (value: number | null) => value === null || Number.isFinite(value);
  const isCriticalValid =
    !hasThreshold(kind) || (criticalValue !== null && Number.isFinite(criticalValue) && criticalValue > 0);
  const isWarningValid = isValid(warningValue);
  const isRecoveryValid = isValid(recoveryValue);

  const readCondition = (): Schemas['Condition'] | undefined => {
    if (!isCriticalValid) {
      return undefined;
    }
    const limit = criticalValue ?? 0;
    switch (kind) {
      case 'offline':
      case 'uptime_down':
      case 'cron_missed':
      case 'cron_failed':
        return { kind };
      case 'process_stopped':
      case 'port_closed':
        return { kind, name: subjectName.trim() || null };
      case 'logs':
        return Number.isInteger(limit)
          ? {
              kind,
              level: logLevel,
              count: limit,
              within_secs: logWithin,
              service: logService || null,
              text: logText.trim() || null,
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
  };
  const readScope = (): Scope | undefined => {
    if (scopeKind === 'label' && noun === 'server') {
      return label ? { kind: 'label', label } : undefined;
    }
    if (scopeKind === 'picked') {
      if (pickedIds.length === 0) {
        return undefined;
      }
      if (noun === 'check') {
        return { kind: 'checks', check_ids: pickedIds };
      }
      return noun === 'cron' ? { kind: 'crons', cron_ids: pickedIds } : { kind: 'servers', server_ids: pickedIds };
    }
    return { kind: 'all' };
  };
  const [remindEvery, remindTimes] = remind === 'off' ? [null, 0] : remind.split(':').map(Number);
  const options: Schemas['RuleOptions'] = {
    aggregation,
    window_secs: hasAggregation(kind) ? windowSecs : 0,
    warning: hasWarning(kind) ? warningValue : null,
    resolve_at: hasThreshold(kind) ? recoveryValue : null,
    resolve_for_secs: resolveForSecs,
    no_data: noun === 'cron' ? 'keep' : noData,
    remind_every_secs: remindEvery,
    remind_times: remindTimes ?? 0,
    runbook_url: runbookUrl.trim() || null,
    severity_label: saved?.severity_label ?? null,
    warning_integration_ids: warningIntegrationIds,
    message_limit: messageLimit === 'off' ? null : Number(messageLimit),
    quiet_hours: quietHours,
    time_zone: timeZone,
  };
  const condition = readCondition();
  const scope = readScope();

  const { data: backtest } = useQuery({
    ...backtestAlertRuleOptions({ condition: condition ?? { kind: 'offline' }, for_secs: forSecs, scope: scope ?? { kind: 'all' } }),
    enabled: !!condition && !!scope,
    placeholderData: keepPreviousData,
  });
  const { data: savedBacktest } = useQuery({
    ...backtestAlertRuleOptions({
      condition: rule?.condition ?? { kind: 'offline' },
      for_secs: rule?.for_secs,
      scope: rule?.scope ?? { kind: 'all' },
    }),
    enabled: !!rule,
  });

  const onSaved = (saved: Schemas['AlertRule']) => {
    queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
    toast.success(rule ? `Saved ${saved.name}` : `Created ${saved.name}`);
    navigate(`/alerts/${saved.id}`);
  };
  const create = useMutation({ ...createAlertRuleOptions(), onSuccess: onSaved, meta: { errorMessage: 'The rule could not be created' } });
  const update = useMutation({ ...updateAlertRuleOptions(), onSuccess: onSaved, meta: { errorMessage: 'The rule could not be saved' } });
  const toggle = useMutation({
    ...updateAlertRuleOptions(),
    meta: { errorMessage: 'The rule could not be updated' },
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
      toast.success(saved.enabled ? 'Alerts are on' : 'Alerts are off');
    },
  });
  const remove = useMutation({
    ...deleteAlertRuleOptions(),
    meta: { errorMessage: 'The rule could not be deleted' },
    onSuccess: () => {
      navigate('/alerts');
      queryClient.invalidateQueries({ queryKey: ['alert-rules', 'list'] });
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      toast.success(`Deleted ${rule?.name}`);
    },
  });
  const test = useMutation({
    ...testAlertRuleOptions(),
    meta: { errorMessage: 'The test alert could not be sent' },
    onSuccess: () => toast.success('Test alert sent'),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['integrations'] }),
  });

  const canSave = !!name.trim() && !!condition && !!scope && isWarningValid && isRecoveryValid;
  const declare = {
    declare_incident: declareIncident,
    declare_severity: declareIncident ? declareSeverity : null,
    declare_hidden_page_ids: declareIncident && declareShown === 'hidden' ? statusPages.map((page) => page.id) : [],
  };
  const snapshot = JSON.stringify([
    name.trim(),
    condition,
    forSecs,
    scope,
    integrationIds,
    options,
    declareIncident,
    declareSeverity,
    declareShown,
  ]);
  const [savedSnapshot] = useState(snapshot);
  const isSaving = create.isPending || update.isPending;
  const backTo = rule ? `/alerts/${rule.id}` : '/alerts';

  const changed = {
    resolve: (hasThreshold(kind) && recovery !== '') || resolveForSecs !== 0,
    noData: noun !== 'cron' && noData !== 'keep',
    remind: remind !== 'off',
    messageLimit: messageLimit !== 'off',
    quietHours: quietHours.length > 0,
    incident: declareIncident,
    runbook: runbookUrl.trim() !== '',
  };
  const changedCount = Object.values(changed).filter(Boolean).length;
  const marksChanges = !!rule && changedCount > 0;
  const [isMoreOpen, setIsMoreOpen] = useState(changedCount > 0);

  const save = () => {
    if (!condition || !scope) {
      return;
    }
    const body = { name: name.trim(), condition, for_secs: forSecs, scope, integration_ids: integrationIds, options, ...declare };
    if (rule) {
      update.mutate({ ruleId: rule.id, body });
    } else {
      create.mutate(body);
    }
  };

  const changeKind = (next: ConditionKind) => {
    if (readTargetNoun(next) !== noun) {
      setScopeKind('all');
      setPickedIds([]);
    }
    setKind(next);
    setCritical(DEFAULT_THRESHOLDS[next] ?? '');
    setWarning('');
    setIsWarningOpen(false);
    setRecovery('');
    setAggregation(next === 'response_time' || next === 'first_byte' ? 'p95' : 'avg');
  };

  const applyTemplate = (template: RuleTemplate) => {
    changeKind(template.kind);
    setName(template.name);
    setCritical(template.critical ?? '');
    setWarning(template.warning ?? '');
    setIsWarningOpen(!!template.warning);
    setAggregation(template.aggregation ?? 'avg');
    setWindowSecs(template.windowSecs ?? 0);
    setSubjectName('');
    setForSecs(template.forSecs);
  };

  const resetMoreOptions = () => {
    setRecovery('');
    setResolveForSecs(0);
    setNoData('keep');
    setRemind('off');
    setMessageLimit('off');
    setQuietHours([]);
    setDeclareIncident(false);
    setRunbookUrl('');
  };

  const matched = targets.filter((target) =>
    scopeKind === 'all' ? true : scopeKind === 'label' ? target.labels.includes(label) : pickedIds.includes(target.id)
  );
  const direction = kind === 'cert_days' ? 'Below' : 'Above';
  const nouns = `${noun[0]!.toUpperCase()}${noun.slice(1)}s`;
  const scopeItems: { value: ScopeKind; label: string }[] = [
    { value: 'all', label: `All ${nouns}` },
    ...(noun === 'server' ? [{ value: 'label' as const, label: 'With Label' }] : []),
    { value: 'picked', label: `Picked ${nouns}` },
  ];

  const windows = [...new Set([...WINDOW_OPTIONS.map((option) => option.value), windowSecs])]
    .filter((secs) => secs > 0)
    .sort((a, b) => a - b);
  const measureGroups = [
    [{ value: 'latest', label: 'Latest reading' }],
    ...AGGREGATION_OPTIONS.map((option) =>
      windows.map((secs) => ({ value: `${option.value}:${secs}`, label: `${option.label} over ${formatSpan(secs)}` }))
    ),
  ];
  const measureValue = windowSecs === 0 ? 'latest' : `${aggregation}:${windowSecs}`;
  const changeMeasure = (value: string | null) => {
    if (!value || value === 'latest') {
      setWindowSecs(0);
      return;
    }
    const [nextAggregation, nextWindow] = value.split(':');
    setAggregation(nextAggregation as Aggregation);
    setWindowSecs(Number(nextWindow));
  };

  const recoveryVerb = kind === 'cert_days' ? 'Over' : 'Under';
  const limitText = criticalValue !== null && isCriticalValid ? formatAmount(criticalValue, unit) : 'the limit';
  const recoveryChoices = [
    ...new Set([...getRecoveryPresets(kind, criticalValue), ...(recoveryValue !== null ? [recoveryValue] : [])]),
  ];
  const describeRecovery = (value: number | null) =>
    `${recoveryVerb} ${value === null ? limitText : formatAmount(value, unit)}`;
  const resolveGroups = hasThreshold(kind)
    ? [null, ...recoveryChoices].map((value) =>
        RESOLVE_OPTIONS.map((option) => ({
          value: `${value ?? 'limit'}:${option.value}`,
          label: option.value === 0 ? describeRecovery(value) : `${describeRecovery(value)} for ${formatSpan(option.value)}`,
        }))
      )
    : [
        RESOLVE_OPTIONS.map((option) => ({
          value: `limit:${option.value}`,
          label:
            option.value === 0
              ? kind === 'offline'
                ? 'As soon as data comes back'
                : 'As soon as it recovers'
              : `${formatSpan(option.value)} after it recovers`,
        })),
      ];
  const resolveValue = `${hasThreshold(kind) && recoveryValue !== null ? recoveryValue : 'limit'}:${resolveForSecs}`;
  const changeResolve = (value: string | null) => {
    const [nextRecovery, nextSecs] = (value ?? 'limit:0').split(':');
    setRecovery(nextRecovery === 'limit' ? '' : (nextRecovery ?? ''));
    setResolveForSecs(Number(nextSecs));
  };

  const describeResolve = () => {
    if (hasThreshold(kind) && recoveryValue !== null) {
      const span = resolveForSecs > 0 ? ` for ${formatSpan(resolveForSecs)}` : '';
      return `Resolves ${describeRecovery(recoveryValue).toLowerCase()}${span}`;
    }
    if (resolveForSecs > 0) {
      return `Resolves ${formatSpan(resolveForSecs)} after it recovers`;
    }
    return kind === 'offline' ? 'Resolves as soon as data comes back' : 'Resolves as soon as it recovers';
  };
  const moreSummary = [
    describeResolve(),
    changed.noData && (noData === 'alert' ? 'Alerts when data stops' : 'Resolves when data stops'),
    remind === 'off'
      ? 'No reminders'
      : `Reminds ${(REMIND_OPTIONS.find((option) => option.value === remind)?.label ?? 'while firing').toLowerCase()}`,
    changed.messageLimit && `At most ${messageLimit} messages an hour`,
    quietHours.length > 0 ? 'Quiet hours on' : 'No quiet hours',
    declareIncident ? 'Opens an incident' : 'No incident',
    changed.runbook && 'Runbook linked',
  ]
    .filter(Boolean)
    .join(' · ');

  const visibleIntegrations = (integrations ?? []).filter(
    (integration) => (levels[integration.id] ?? 'off') !== 'off' || shownIntegrationIds.includes(integration.id)
  );
  const hiddenCount = (integrations ?? []).length - visibleIntegrations.length;

  const now = Date.now();
  const firedCount = history.filter((alert) => now - new Date(alert.started_at).getTime() < RECENT_MS).length;
  const forField = (
    <SelectField
      label={kind === 'offline' ? 'No Data For' : 'For'}
      value={forSecs.toString()}
      onValueChange={(value) => setForSecs(Number(value))}
      items={FOR_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
    />
  );

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo={backTo} icon="bell" title={rule ? `Alerts / ${rule.name}` : 'Alerts'}>
        {!rule && (
          <>
            <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={backTo} />}>
              Cancel
            </Button>
            <Button className="w-fit" disabled={!canSave || isSaving} onClick={() => formRef.current?.requestSubmit()}>
              {isSaving && <Icon name="loader" className="animate-spin" />}
              Create Rule
            </Button>
          </>
        )}
      </PageBar>
      {rule && (
        <SaveBar
          isDirty={snapshot !== savedSnapshot}
          isSaving={isSaving}
          canSave={canSave}
          onSave={() => formRef.current?.requestSubmit()}
          onDiscard={onDiscard}
        />
      )}

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:px-4">
        <form
          ref={formRef}
          onSubmit={(event) => {
            event.preventDefault();
            if (canSave) {
              save();
            }
          }}
          className="flex w-200 max-w-full min-w-0 flex-col gap-8"
        >
          <div className="flex flex-col gap-1">
            <h1 className="text-title-page font-semibold">{rule ? `Edit ${rule.name}` : 'New Rule'}</h1>
            <p className="text-body text-text-secondary">
              {rule
                ? `Created ${formatDate(rule.created_at)}. Fired ${firedCount === 1 ? 'once' : `${firedCount} times`} in the last 30 days.`
                : 'Pick a condition and a limit. Everything else starts on a sensible default.'}
            </p>
          </div>

          <FormSection
            title="Rule"
            action={
              hasWarning(kind) && (
                <Button
                  type="button"
                  variant="tertiary"
                  className="w-fit"
                  onClick={() => {
                    if (isWarningOpen) {
                      setWarning('');
                    }
                    setIsWarningOpen(!isWarningOpen);
                  }}
                >
                  {isWarningOpen ? 'Remove Warning Level' : 'Add Warning Level'}
                </Button>
              )
            }
          >
            <div className="flex flex-col gap-3.5 p-3.5">
              <FieldRow label="Name">
                {({ inputId }) => (
                  <Input
                    id={inputId}
                    value={name}
                    onValueChange={setName}
                    placeholder="High CPU"
                    maxLength={100}
                    required
                    autoFocus={!rule}
                  />
                )}
              </FieldRow>
              <SelectField
                label="Condition"
                value={kind}
                onValueChange={(value) => changeKind(value as ConditionKind)}
                groups={METRIC_GROUPS.map((group) => ({
                  label: group.label,
                  items: group.items.map((item) => ({ value: item.kind, label: CONDITION_LABELS[item.kind] })),
                }))}
              />
              {kind === 'logs' ? (
                <>
                  <FieldGrid hint="Only processes with logs: true send lines.">
                    <SelectField
                      label="Level"
                      value={logLevel}
                      onValueChange={(value) => setLogLevel(value ?? 'error')}
                      items={LOG_LEVEL_OPTIONS}
                    />
                    <SelectField
                      label="Source"
                      value={logService || 'any'}
                      onValueChange={(value) => setLogService(!value || value === 'any' ? '' : value)}
                      items={[{ value: 'any', label: 'Any source' }, ...logServices.map((service) => ({ value: service, label: service }))]}
                    />
                  </FieldGrid>
                  <FieldRow label="Text" hint="Optional. Matches lines that contain this text.">
                    {({ inputId, hintId }) => (
                      <Input
                        id={inputId}
                        aria-describedby={hintId}
                        value={logText}
                        onValueChange={setLogText}
                        placeholder="Any text"
                        maxLength={200}
                      />
                    )}
                  </FieldRow>
                  <FieldGrid
                    columns={3}
                    hint={
                      isCriticalValid
                        ? `Fires when ${critical} or more matching lines arrive within ${formatSpan(logWithin)}.`
                        : undefined
                    }
                  >
                    <UnitField
                      label="At Least"
                      unit={unit}
                      value={critical}
                      onValueChange={setCritical}
                      isInvalid={!isCriticalValid}
                    />
                    <SelectField
                      label="Within"
                      value={logWithin.toString()}
                      onValueChange={(value) => setLogWithin(Number(value))}
                      items={LOG_WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
                    />
                    {forField}
                  </FieldGrid>
                </>
              ) : hasThreshold(kind) ? (
                <FieldGrid columns={isWarningOpen ? 3 : 2}>
                  <UnitField
                    label={isWarningOpen ? `Critical ${direction}` : direction}
                    unit={unit}
                    value={critical}
                    onValueChange={setCritical}
                    isInvalid={!isCriticalValid}
                  />
                  {isWarningOpen && (
                    <UnitField
                      label={`Warning ${direction}`}
                      unit={unit}
                      value={warning}
                      onValueChange={setWarning}
                      placeholder="Off"
                      isInvalid={!isWarningValid}
                    />
                  )}
                  {forField}
                </FieldGrid>
              ) : hasSubjectName(kind) ? (
                <FieldGrid>
                  <FieldRow label={kind === 'process_stopped' ? 'Process' : 'Port'}>
                    {({ inputId }) => (
                      <Input
                        id={inputId}
                        value={subjectName}
                        onValueChange={setSubjectName}
                        placeholder={kind === 'process_stopped' ? 'Any watched process' : 'Any watched port'}
                      />
                    )}
                  </FieldRow>
                  {forField}
                </FieldGrid>
              ) : (
                forField
              )}
            </div>
            {hasAggregation(kind) && (
              <SettingRow
                title="Measured As"
                description="How readings are combined before the check"
                className="py-3.5"
                control={(labelId) => (
                  <PlainSelect labelId={labelId} value={measureValue} onValueChange={changeMeasure} groups={measureGroups} />
                )}
              />
            )}
          </FormSection>

          <FormSection title={nouns}>
            <SettingRow
              title="Scope"
              isCaption
              description={
                scopeKind === 'picked'
                  ? `Only the ${noun}s you pick.`
                  : `${nouns} added later join the rule when they match.`
              }
              actions={
                <div role="radiogroup" aria-label="Scope" className="flex shrink-0 items-center gap-0.5">
                  {scopeItems.map((item) => (
                    <button
                      key={item.value}
                      type="button"
                      role="radio"
                      aria-checked={scopeKind === item.value}
                      onClick={() => setScopeKind(item.value)}
                      className="h-7 rounded-sm px-2.5 text-body font-medium whitespace-nowrap text-text-secondary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 aria-checked:bg-surface-fill aria-checked:text-text-primary"
                    >
                      {item.label}
                    </button>
                  ))}
                </div>
              }
            />
            {scopeKind === 'picked' ? (
              <div className="flex flex-col py-1">
                {targets.length === 0 && <p className="px-3.5 py-2.5 text-body text-text-secondary">No {noun}s yet.</p>}
                {targets.map((target) => (
                  <label key={target.id} className="flex cursor-pointer items-center gap-3 px-3.5 py-2.5 hover:bg-surface-subtle">
                    <Checkbox
                      checked={pickedIds.includes(target.id)}
                      onCheckedChange={(checked) =>
                        setPickedIds(checked ? [...pickedIds, target.id] : pickedIds.filter((id) => id !== target.id))
                      }
                    />
                    <span className="flex-1 text-body">{target.name}</span>
                    <span className="truncate text-caption font-normal text-text-tertiary">{target.detail}</span>
                  </label>
                ))}
              </div>
            ) : (
              <div className="flex flex-col gap-3.5 p-3.5">
                {scopeKind === 'label' ? (
                  <SelectField
                    label="Label"
                    placeholder={labels.length > 0 ? 'Pick a label' : 'No server has a label yet'}
                    value={label || null}
                    onValueChange={(value) => setLabel(value ?? '')}
                    items={labels.map((item) => ({ value: item, label: item }))}
                    hint={describeMatches(matched.map((target) => target.name), noun)}
                  />
                ) : (
                  <p className="text-caption leading-4.5 text-text-secondary">{describeMatches(matched.map((target) => target.name), noun)}</p>
                )}
              </div>
            )}
          </FormSection>

          <FormSection
            title="Notify"
            description="Pick what each integration gets. Every one also hears when the alert resolves."
            action={
              <Button
                type="button"
                variant="tertiary"
                className="w-fit"
                disabled={!condition || integrationIds.length === 0 || test.isPending}
                onClick={() =>
                  condition && test.mutate({ name: name.trim() || 'Untitled Rule', condition, for_secs: forSecs, integration_ids: integrationIds })
                }
              >
                {test.isPending && <Icon name="loader" className="animate-spin" />}
                Send Test Alert
              </Button>
            }
          >
            {visibleIntegrations.map((integration) => (
              <NotifyRow
                key={integration.id}
                integration={integration}
                value={levels[integration.id] ?? 'off'}
                onValueChange={(value) => {
                  setShownIntegrationIds([...shownIntegrationIds, integration.id]);
                  setNotifyLevels({ ...levels, [integration.id]: value });
                }}
              />
            ))}
            <button
              type="button"
              onClick={() =>
                hiddenCount > 0
                  ? setShownIntegrationIds((integrations ?? []).map((integration) => integration.id))
                  : setIsAddOpen(true)
              }
              className="flex items-center gap-2 bg-surface-subtle px-3.5 py-2.5 text-left text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
            >
              <Icon name="plus" className="size-3.5" />
              Add Integration
            </button>
          </FormSection>

          <FormSection
            title="More Options"
            description={
              isMoreOpen && marksChanges
                ? `${changedCount === 1 ? '1 setting differs' : `${changedCount} settings differ`} from the defaults and ${changedCount === 1 ? 'is' : 'are'} marked. Reset puts ${changedCount === 1 ? 'it' : 'them'} back.`
                : 'Resolving, reminders, quiet hours, incidents and a runbook link. The defaults suit most rules.'
            }
            action={
              isMoreOpen && (
                <div className="flex gap-2">
                  <Button type="button" variant="tertiary" className="w-fit" disabled={changedCount === 0} onClick={resetMoreOptions}>
                    Reset to Defaults
                  </Button>
                  <Button type="button" variant="tertiary" className="w-fit" onClick={() => setIsMoreOpen(false)}>
                    Hide
                  </Button>
                </div>
              )
            }
          >
            {!isMoreOpen ? (
              <div className="flex items-center gap-3 px-3.5 py-3">
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-body font-medium">{changedCount === 0 ? 'Using Defaults' : 'Customized'}</span>
                  <span className="text-body text-text-secondary">{moreSummary}</span>
                </div>
                <Button type="button" variant="tertiary" className="w-fit" onClick={() => setIsMoreOpen(true)}>
                  Show
                </Button>
              </div>
            ) : (
              <>
                <SettingRow
                  title="Resolve When"
                  description={
                    hasThreshold(kind)
                      ? `How long it must stay ${kind === 'cert_days' ? 'over' : 'under'} the limit before it resolves`
                      : 'How long it must stay healthy before it resolves'
                  }
                  isChanged={!!rule && changed.resolve}
                  control={(labelId) => (
                    <PlainSelect labelId={labelId} value={resolveValue} onValueChange={changeResolve} groups={resolveGroups} />
                  )}
                />
                {noun !== 'cron' && (
                  <SettingRow
                    title="If Data Stops"
                    description="When an agent disconnects or a check is paused"
                    isChanged={!!rule && changed.noData}
                    control={(labelId) => (
                      <PlainSelect
                        labelId={labelId}
                        value={noData}
                        onValueChange={(value) => setNoData((value ?? 'keep') as NoData)}
                        items={NO_DATA_OPTIONS}
                      />
                    )}
                  />
                )}
                <SettingRow
                  title="Remind"
                  description="While it is still firing"
                  isChanged={!!rule && changed.remind}
                  control={(labelId) => (
                    <PlainSelect labelId={labelId} value={remind} onValueChange={(value) => setRemind(value ?? 'off')} items={REMIND_OPTIONS} />
                  )}
                />
                <SettingRow
                  title="Message Limit"
                  description="Past it, one summary goes out and the rule stays quiet for the hour"
                  isChanged={!!rule && changed.messageLimit}
                  control={(labelId) => (
                    <PlainSelect
                      labelId={labelId}
                      value={messageLimit}
                      onValueChange={(value) => setMessageLimit(value ?? 'off')}
                      items={MESSAGE_LIMIT_OPTIONS}
                    />
                  )}
                />
                <SettingRow
                  title="Quiet Hours"
                  description="No messages during planned work, like a nightly backup"
                  isChanged={!!rule && changed.quietHours}
                  actions={
                    quietHours.length === 0 ? (
                      <Button type="button" variant="tertiary" className="w-fit" onClick={() => setQuietHours([DEFAULT_QUIET_WINDOW])}>
                        Add Quiet Hours
                      </Button>
                    ) : (
                      <div className="flex gap-2">
                        {quietHours.length < 5 && (
                          <Button
                            type="button"
                            variant="tertiary"
                            className="w-fit"
                            onClick={() => setQuietHours([...quietHours, DEFAULT_QUIET_WINDOW])}
                          >
                            Add Another Window
                          </Button>
                        )}
                        <Button type="button" variant="tertiary" className="w-fit" onClick={() => setQuietHours([])}>
                          Remove Quiet Hours
                        </Button>
                      </div>
                    )
                  }
                >
                  {quietHours.length > 0 && (
                    <QuietHoursField
                      windows={quietHours}
                      timeZone={timeZone}
                      onWindowsChange={setQuietHours}
                      onTimeZoneChange={setTimeZone}
                    />
                  )}
                </SettingRow>
                <SettingRow
                  title="Open an Incident"
                  description="For bad outages, so the team works from one timeline"
                  isChanged={!!rule && changed.incident}
                  actions={(labelId) => (
                    <Switch.Root aria-labelledby={labelId} checked={declareIncident} onCheckedChange={setDeclareIncident}>
                      <Switch.Thumb />
                    </Switch.Root>
                  )}
                >
                  {declareIncident && (
                    <FieldGrid hint="Daroyan declares it, links this alert and adds what it fires on as an affected component.">
                      <SelectField
                        label="Severity"
                        value={declareSeverity}
                        onValueChange={(value) => setDeclareSeverity(value ?? 'major')}
                        items={DECLARE_SEVERITY_OPTIONS}
                      />
                      <SelectField
                        label="Show on Status Pages"
                        value={declareShown}
                        onValueChange={(value) => setDeclareShown(value ?? 'shown')}
                        items={DECLARE_SHOWN_OPTIONS}
                      />
                    </FieldGrid>
                  )}
                </SettingRow>
                <SettingRow
                  title="Runbook URL"
                  description="Linked in every message, so whoever is on call knows what to do"
                  isChanged={!!rule && changed.runbook}
                  control={(labelId) => (
                    <Input
                      aria-labelledby={labelId}
                      type="url"
                      value={runbookUrl}
                      onValueChange={setRunbookUrl}
                      placeholder="https://wiki.example.com/runbooks/api"
                    />
                  )}
                />
              </>
            )}
          </FormSection>
        </form>

        <aside aria-label="Preview" className="flex w-65 shrink-0 flex-col gap-5 pt-2 max-lg:hidden">
          <div className="flex flex-col gap-3">
            <div className="flex items-center justify-between gap-3 text-body">
              <h2 className="font-medium">{rule ? 'With These Changes' : 'Preview'}</h2>
              <span className="text-text-tertiary">Last {backtest?.days ?? 7} days</span>
            </div>
            <BacktestBox
              backtest={condition && scope ? backtest : undefined}
              savedCount={rule && savedBacktest?.is_supported ? savedBacktest.hits.length : undefined}
            />
          </div>
          {rule ? (
            <>
              <div className="h-px bg-border-row" />
              <div className="flex flex-col gap-2.5 text-body">
                <h2 className="font-medium">Status</h2>
                <div className="flex items-center gap-3">
                  <span id="send-alerts" className="flex-1 text-text-secondary">
                    Send alerts
                  </span>
                  <Switch.Root
                    aria-labelledby="send-alerts"
                    checked={rule.enabled}
                    disabled={toggle.isPending}
                    onCheckedChange={(enabled) => toggle.mutate({ ruleId: rule.id, body: { enabled } })}
                  >
                    <Switch.Thumb />
                  </Switch.Root>
                </div>
                <p className="text-text-secondary">Turn off to pause the rule without losing it</p>
              </div>
              {isAdmin && (
                <>
                  <div className="h-px bg-border-row" />
                  <div className="flex flex-col gap-2.5">
                    <h2 className="text-body font-medium">Delete Rule</h2>
                    <Button
                      type="button"
                      variant="secondary"
                      className="w-fit text-tone-critical-fg"
                      onClick={() => setIsDeleteOpen(true)}
                    >
                      Delete Rule
                    </Button>
                  </div>
                </>
              )}
            </>
          ) : (
            <>
              <div className="h-px bg-border-row" />
              <div className="flex flex-col gap-2.5">
                <h2 className="text-body font-medium">Start From a Template</h2>
                <div className="flex flex-wrap gap-1.5">
                  {TEMPLATES.map((template) => (
                    <button
                      key={template.label}
                      type="button"
                      onClick={() => applyTemplate(template)}
                      className="flex h-5.5 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary outline-none hover:bg-surface-fill-strong hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
                    >
                      {template.label}
                    </button>
                  ))}
                </div>
              </div>
            </>
          )}
        </aside>
      </div>

      <IntegrationDialog
        open={isAddOpen}
        onOpenChange={setIsAddOpen}
        onSaved={(saved) => {
          setShownIntegrationIds([...shownIntegrationIds, saved.id]);
          setNotifyLevels({ ...levels, [saved.id]: 'both' });
        }}
      />
      {rule && (
        <ConfirmDialog
          open={isDeleteOpen}
          onOpenChange={setIsDeleteOpen}
          title={`Delete ${rule.name}?`}
          description="Its history goes too. A firing alert stops without a resolve message."
          confirmLabel="Delete Rule"
          isPending={remove.isPending}
          onConfirm={() => remove.mutate({ ruleId: rule.id })}
        />
      )}
    </div>
  );
}

const DECLARE_SEVERITY_OPTIONS = [
  { value: 'minor', label: 'Minor' },
  { value: 'major', label: 'Major' },
  { value: 'critical', label: 'Critical' },
];

const DECLARE_SHOWN_OPTIONS = [
  { value: 'shown', label: 'Yes, as an affected component' },
  { value: 'hidden', label: 'No, internal until someone posts' },
];

function describeMatches(names: string[], noun: string) {
  if (names.length === 0) {
    return `Matches no ${noun}s yet.`;
  }
  const count = names.length === 1 ? `1 ${noun}` : `${names.length} ${noun}s`;
  return `Matches ${count}: ${joinNames(names)}.`;
}

type FieldGridProps = {
  hint?: string;
  columns?: 2 | 3;
  children: ReactNode;
};

function FieldGrid(props: FieldGridProps) {
  const { hint, columns = 2, children } = props;

  return (
    <div className="flex flex-col gap-1.5">
      <div className={cn('grid grid-cols-1 gap-4', columns === 3 ? 'sm:grid-cols-3' : 'sm:grid-cols-2')}>{children}</div>
      {hint && <p className="text-caption leading-4.5 text-text-secondary">{hint}</p>}
    </div>
  );
}

type UnitFieldProps = {
  label: string;
  unit: string;
  value: string;
  onValueChange: (value: string) => void;
  placeholder?: string;
  isInvalid: boolean;
};

function UnitField(props: UnitFieldProps) {
  const { label, ...rest } = props;

  return <FieldRow label={label}>{({ inputId }) => <UnitInput id={inputId} {...rest} />}</FieldRow>;
}

type UnitInputProps = Omit<UnitFieldProps, 'label'> & {
  id?: string;
};

function UnitInput(props: UnitInputProps) {
  const { id, unit, value, onValueChange, placeholder, isInvalid } = props;

  return (
    <div className="relative">
      <Input
        id={id}
        inputMode="decimal"
        value={value}
        onValueChange={(next) => onValueChange(next.replace(/[^0-9.]/g, ''))}
        placeholder={placeholder}
        aria-invalid={isInvalid || undefined}
      />
      {unit && value && (
        <span aria-hidden className="pointer-events-none absolute inset-y-0 left-px flex items-center pl-3 text-body">
          <span className="invisible whitespace-pre">{value}</span>
          <span className="text-text-primary">{unit === '%' ? unit : ` ${unit}`}</span>
        </span>
      )}
    </div>
  );
}

type SettingRowProps = {
  title: string;
  description: string;
  isCaption?: boolean;
  isChanged?: boolean;
  className?: string;
  control?: (labelId: string) => ReactNode;
  actions?: ReactNode | ((labelId: string) => ReactNode);
  children?: ReactNode;
};

function SettingRow(props: SettingRowProps) {
  const { title, description, isCaption, isChanged, className, control, actions, children } = props;

  const labelId = useId();

  return (
    <div className={cn('flex flex-col gap-3.5 px-3.5 py-3', children && 'pb-3.5', className)}>
      <div className="flex items-center gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <span id={labelId} className="text-body font-medium">
              {title}
            </span>
            {isChanged && <Badge tone="info">Changed</Badge>}
          </span>
          <span className={cn('text-text-secondary', isCaption ? 'text-caption leading-4.5' : 'text-body')}>{description}</span>
        </div>
        {control && <div className="w-60 shrink-0">{control(labelId)}</div>}
        {typeof actions === 'function' ? actions(labelId) : actions}
      </div>
      {children}
    </div>
  );
}

type PlainSelectProps = {
  labelId: string;
  value: string;
  onValueChange: (value: string | null) => void;
  items?: SelectItemData[];
  groups?: SelectItemData[][];
};

function PlainSelect(props: PlainSelectProps) {
  const { labelId, value, onValueChange, items, groups } = props;

  const sets = groups ?? [items ?? []];

  return (
    <Select.Root items={sets.flat()} value={value} onValueChange={(next) => onValueChange(next as string | null)}>
      <Select.Trigger aria-labelledby={labelId}>
        <Select.Value />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner>
          <Select.Popup>
            {sets.map((set, index) => (
              <Fragment key={index}>
                {index > 0 && <Select.Separator />}
                {set.map((item) => (
                  <Select.Item key={item.value} value={item.value}>
                    {item.label}
                  </Select.Item>
                ))}
              </Fragment>
            ))}
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

type FormSectionProps = {
  title: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
};

export function FormSection(props: FormSectionProps) {
  const { title, description, action, children } = props;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h2 className="text-title-section font-medium">{title}</h2>
          {description && <p className="text-body text-text-secondary">{description}</p>}
        </div>
        {action}
      </div>
      <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        {children}
      </div>
    </section>
  );
}

type SelectItemData = { value: string; label: string };

type SelectFieldProps = {
  label: string;
  hint?: string;
  placeholder?: string;
  value: string | null;
  onValueChange: (value: string | null) => void;
  items?: SelectItemData[];
  groups?: { label: string; items: SelectItemData[] }[];
  separatorBefore?: string;
  className?: string;
};

export function SelectField(props: SelectFieldProps) {
  const { label, hint, placeholder, value, onValueChange, items, groups, separatorBefore, className } = props;

  const labelId = useId();
  const hintId = useId();
  const allItems = groups ? groups.flatMap((group) => group.items) : (items ?? []);

  return (
    <div className={cn('flex min-w-0 flex-col gap-1.5', className)}>
      <span id={labelId} className="text-body font-medium">
        {label}
      </span>
      <Select.Root items={allItems} value={value} onValueChange={(next) => onValueChange(next as string | null)}>
        <Select.Trigger aria-labelledby={labelId} aria-describedby={hint ? hintId : undefined}>
          <Select.Value placeholder={placeholder} />
        </Select.Trigger>
        <Select.Portal>
          <Select.Positioner>
            <Select.Popup>
              {groups
                ? groups.map((group, index) => (
                    <Fragment key={group.label}>
                      {index > 0 && <Select.Separator />}
                      <Select.Group>
                        <Select.GroupLabel>{group.label}</Select.GroupLabel>
                        {group.items.map((item) => (
                          <Select.Item key={item.value} value={item.value}>
                            {item.label}
                          </Select.Item>
                        ))}
                      </Select.Group>
                    </Fragment>
                  ))
                : allItems.map((item) => (
                    <Fragment key={item.value}>
                      {item.value === separatorBefore && <Select.Separator />}
                      <Select.Item value={item.value}>{item.label}</Select.Item>
                    </Fragment>
                  ))}
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
      {hint && (
        <p id={hintId} className="text-caption text-text-secondary">
          {hint}
        </p>
      )}
    </div>
  );
}

type NotifyRowProps = {
  integration: Schemas['Integration'];
  value: NotifyLevel;
  onValueChange: (value: NotifyLevel) => void;
};

function NotifyRow(props: NotifyRowProps) {
  const { integration, value, onValueChange } = props;

  const titleId = useId();
  const icon = isIntegrationKind(integration.kind) ? INTEGRATION_KINDS[integration.kind].icon : 'webhook';

  return (
    <div className="flex items-center gap-3 px-3.5 py-2.5">
      <Icon name={icon} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={titleId} className="truncate text-body font-medium">
          {integration.name}
        </span>
        <span className="truncate text-body text-text-secondary">
          {getIntegrationLabel(integration.kind)} · {integration.target}
        </span>
      </div>
      <div className="w-55 shrink-0">
        <PlainSelect labelId={titleId} value={value} onValueChange={(next) => onValueChange((next ?? 'off') as NotifyLevel)} items={NOTIFY_OPTIONS} />
      </div>
    </div>
  );
}

type BacktestBoxProps = {
  backtest?: Schemas['AlertRuleBacktest'];
  savedCount?: number;
};

function BacktestBox(props: BacktestBoxProps) {
  const { backtest, savedCount } = props;

  const count = backtest?.hits.length ?? 0;
  const difference = savedCount === undefined ? 0 : count - savedCount;
  const compared = difference === 0 ? '' : `, ${Math.abs(difference)} ${difference < 0 ? 'fewer' : 'more'}`;

  return (
    <div className="flex flex-col gap-2 rounded-2xl bg-surface-subtle p-4 text-body">
      {!backtest ? (
        <span className="text-text-secondary">Finish the condition to see how often it would have fired.</span>
      ) : !backtest.is_supported ? (
        <span className="text-text-secondary">There is no history to check this rule against yet.</span>
      ) : (
        <>
          <span className="font-medium">
            {count === 0 ? 'Would not have fired' : `Would have fired ${count === 1 ? 'once' : `${count} times`}`}
            {compared}
          </span>
          {backtest.hits.slice(0, 3).map((hit) => (
            <div key={`${hit.server_id}-${hit.at}`} className="flex items-center gap-3">
              <span className="min-w-0 flex-1 truncate text-text-secondary">
                {hit.server_name} ·{' '}
                {hit.subject === 'CPU' || hit.subject === 'Memory' || hit.subject === 'Load' ? hit.value : hit.subject}
              </span>
              <span className="shrink-0 text-text-tertiary">{formatDate(hit.at)}</span>
            </div>
          ))}
        </>
      )}
    </div>
  );
}
