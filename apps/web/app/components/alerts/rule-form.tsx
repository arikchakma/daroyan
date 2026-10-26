import type { Schemas } from '@daroyan/client';
import { keepPreviousData, useMutation, useQuery } from '@tanstack/react-query';
import { Fragment, useId, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { Link, useNavigate } from 'react-router';

import { IntegrationDialog } from '~/components/alerts/integration-dialog';
import { getIntegrationLabel } from '~/components/alerts/integration-logo';
import { DEFAULT_QUIET_WINDOW, getBrowserTimeZone, QuietHoursField } from '~/components/alerts/quiet-hours-field';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Checkbox } from '~/components/ui/checkbox';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { Select } from '~/components/ui/select';
import { Switch } from '~/components/ui/switch';
import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  backtestAlertRuleOptions,
  createAlertRuleOptions,
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
  CONDITION_OPTIONS,
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
  SEVERITY_LABEL_OPTIONS,
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

function RuleFormBody(props: RuleFormBodyProps) {
  const { rule, draft, onDiscard } = props;

  const formRef = useRef<HTMLFormElement>(null);
  const navigate = useNavigate();
  const { data: servers } = useQuery(listServersOptions());
  const { data: checks } = useQuery(listUptimeChecksOptions());
  const { data: crons } = useQuery({ queryKey: ['crons'], queryFn: () => request(apiClient.GET('/v1/crons')) });
  const { data: integrations } = useQuery(listIntegrationsOptions());
  const { data: logSources } = useQuery(logSourcesOptions());
  const saved = rule?.options;
  const initial = rule?.condition ?? draft?.condition;

  const [name, setName] = useState(rule?.name ?? draft?.name ?? '');
  const [runbookUrl, setRunbookUrl] = useState(saved?.runbook_url ?? '');
  const [severityLabel, setSeverityLabel] = useState(saved?.severity_label ?? 'none');
  const [kind, setKind] = useState<ConditionKind>(initial?.kind ?? 'cpu');
  const [critical, setCritical] = useState(
    initial ? (readThreshold(initial)?.toString() ?? '') : (DEFAULT_THRESHOLDS.cpu ?? '')
  );
  const [warning, setWarning] = useState(saved?.warning?.toString() ?? '');
  const [recovery, setRecovery] = useState(saved?.resolve_at?.toString() ?? '');
  const [aggregation, setAggregation] = useState<Aggregation>(saved?.aggregation ?? 'avg');
  const [windowSecs, setWindowSecs] = useState(saved?.window_secs ?? 0);
  const [forSecs, setForSecs] = useState(rule?.for_secs ?? draft?.forSecs ?? 300);
  const [resolveForSecs, setResolveForSecs] = useState(saved?.resolve_for_secs ?? 0);
  const [noData, setNoData] = useState<NoData>(saved?.no_data ?? 'keep');
  const [remind, setRemind] = useState(
    saved?.remind_every_secs ? `${saved.remind_every_secs}:${saved.remind_times ?? 1}` : 'off'
  );
  const [messageLimit, setMessageLimit] = useState(saved?.message_limit?.toString() ?? 'off');
  const [quietHours, setQuietHours] = useState(saved?.quiet_hours ?? []);
  const [isNoiseOpen, setIsNoiseOpen] = useState(
    (saved?.no_data ?? 'keep') !== 'keep' || !!saved?.message_limit || (saved?.quiet_hours ?? []).length > 0
  );
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
  const [isAddOpen, setIsAddOpen] = useState(false);
  const [declareIncident, setDeclareIncident] = useState(rule?.declare_incident ?? false);
  const [declareSeverity, setDeclareSeverity] = useState(rule?.declare_severity ?? 'major');
  const [declareShown, setDeclareShown] = useState(
    rule && rule.declare_hidden_page_ids.length > 0 ? 'hidden' : 'shown'
  );
  const { data: statusPages = [] } = useQuery(listStatusPagesOptions());
  const declareIncidentId = useId();

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
    warning: hasThreshold(kind) && kind !== 'logs' ? warningValue : null,
    resolve_at: hasThreshold(kind) ? recoveryValue : null,
    resolve_for_secs: resolveForSecs,
    no_data: noun === 'cron' ? 'keep' : noData,
    remind_every_secs: remindEvery,
    remind_times: remindTimes ?? 0,
    runbook_url: runbookUrl.trim() || null,
    severity_label: severityLabel === 'none' ? null : severityLabel,
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

  const onSaved = (saved: Schemas['AlertRule']) => {
    queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
    toast.success(rule ? `Saved ${saved.name}` : `Created ${saved.name}`);
    navigate(`/alerts/${saved.id}`);
  };
  const create = useMutation({ ...createAlertRuleOptions(), onSuccess: onSaved, meta: { errorMessage: 'The rule could not be created' } });
  const update = useMutation({ ...updateAlertRuleOptions(), onSuccess: onSaved, meta: { errorMessage: 'The rule could not be saved' } });
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
    setRecovery('');
    setAggregation(next === 'response_time' || next === 'first_byte' ? 'p95' : 'avg');
  };

  const applyTemplate = (template: RuleTemplate) => {
    changeKind(template.kind);
    setName(template.name);
    setCritical(template.critical ?? '');
    setWarning(template.warning ?? '');
    setAggregation(template.aggregation ?? 'avg');
    setWindowSecs(template.windowSecs ?? 0);
    setSubjectName('');
    setForSecs(template.forSecs);
  };

  const matched = targets.filter((target) =>
    scopeKind === 'all' ? true : scopeKind === 'label' ? target.labels.includes(label) : pickedIds.includes(target.id)
  );
  const direction = kind === 'cert_days' ? 'Below' : 'Above';
  const describeCompare = () => {
    if (kind === 'logs') {
      return 'Fires when the count of matching lines reaches the critical limit.';
    }
    const metric = CONDITION_OPTIONS.find((option) => option.kind === kind)?.label.toLowerCase() ?? 'value';
    if (!hasAggregation(kind) || windowSecs === 0) {
      return `The latest ${metric} is compared with these.`;
    }
    const reduce = AGGREGATION_OPTIONS.find((option) => option.value === aggregation)?.label ?? '';
    const window = WINDOW_OPTIONS.find((option) => option.value === windowSecs)?.label ?? '';
    return `The ${reduce === 'p95' ? reduce : reduce.toLowerCase()} ${metric} over the last ${window} is compared with these.`;
  };
  const scopeItems = [
    { value: 'all', label: `All ${noun}s` },
    ...(noun === 'server' ? [{ value: 'label', label: 'With label' }] : []),
    { value: 'picked', label: `Picked ${noun}s` },
  ];

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo={backTo} icon="bell" title="Alerts">
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
            <h1 className="text-title-page font-semibold">{rule ? 'Edit Rule' : 'New Rule'}</h1>
            <p className="text-body text-text-secondary">
              Watch a server, uptime check, cron job or log stream, and tell your team when it crosses a limit.
            </p>
          </div>

          <FormSection title="Rule">
            <div className="flex flex-col gap-5 p-4">
              <FieldRow label="Name" hint="No need to be unique, just a name your team will recognize.">
                {({ inputId, hintId }) => (
                  <Input
                    id={inputId}
                    aria-describedby={hintId}
                    value={name}
                    onValueChange={setName}
                    placeholder="High CPU"
                    maxLength={100}
                    required
                    autoFocus={!rule}
                  />
                )}
              </FieldRow>
              <FieldGrid hint="Both go into every message, so whoever is on call knows what to do.">
                <FieldRow label="Runbook URL">
                  {({ inputId }) => (
                    <Input
                      id={inputId}
                      type="url"
                      value={runbookUrl}
                      onValueChange={setRunbookUrl}
                      placeholder="https://wiki.example.com/runbooks/api"
                    />
                  )}
                </FieldRow>
                <SelectField
                  label="Severity Label"
                  value={severityLabel}
                  onValueChange={(value) => setSeverityLabel(value ?? 'none')}
                  items={SEVERITY_LABEL_OPTIONS}
                />
              </FieldGrid>
            </div>
          </FormSection>

          <FormSection title="Monitor" description="What to measure, and how to reduce it to one number.">
            <div className="flex flex-col gap-5 p-4">
              <FieldGrid hint="Grouped by source: Servers, Uptime checks, Crons and Logs. The scope below follows the source.">
                <SelectField
                  label="Metric"
                  value={kind}
                  onValueChange={(value) => changeKind(value as ConditionKind)}
                  groups={METRIC_GROUPS.map((group) => ({
                    label: group.label,
                    items: group.items.map((item) => ({ value: item.kind, label: item.label })),
                  }))}
                />
                {hasAggregation(kind) ? (
                  <SelectField
                    label="Aggregation"
                    value={aggregation}
                    onValueChange={(value) => setAggregation((value ?? 'avg') as Aggregation)}
                    items={AGGREGATION_OPTIONS}
                  />
                ) : kind === 'logs' ? (
                  <SelectField
                    label="Level"
                    value={logLevel}
                    onValueChange={(value) => setLogLevel(value ?? 'error')}
                    items={LOG_LEVEL_OPTIONS}
                  />
                ) : hasSubjectName(kind) ? (
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
                ) : (
                  <div />
                )}
              </FieldGrid>
              {hasAggregation(kind) && (
                <FieldGrid
                  hint={
                    windowSecs === 0
                      ? 'Uses the latest sample. Pick a window to smooth out spikes.'
                      : `Each ${noun} fires and resolves on its own.`
                  }
                >
                  <SelectField
                    label="Over the Last"
                    value={windowSecs.toString()}
                    onValueChange={(value) => setWindowSecs(Number(value))}
                    items={WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
                  />
                  <div />
                </FieldGrid>
              )}
              {kind === 'logs' && (
                <FieldGrid hint="Only processes with logs: true send lines.">
                  <SelectField
                    label="Source"
                    value={logService || 'any'}
                    onValueChange={(value) => setLogService(!value || value === 'any' ? '' : value)}
                    items={[{ value: 'any', label: 'Any source' }, ...logServices.map((service) => ({ value: service, label: service }))]}
                  />
                  <FieldRow label="Text">
                    {({ inputId }) => (
                      <Input id={inputId} value={logText} onValueChange={setLogText} placeholder="Any text" maxLength={200} />
                    )}
                  </FieldRow>
                </FieldGrid>
              )}
            </div>
          </FormSection>

          {hasThreshold(kind) && (
            <FormSection
              title="Thresholds"
              description={
                kind === 'logs'
                  ? 'Critical is required. Count the matching lines within a window.'
                  : 'Critical is required. Warning is optional and only goes to integrations set to get it.'
              }
            >
              <div className="p-4">
                <FieldGrid hint={describeCompare()}>
                  <UnitField
                    label={`Critical ${direction}`}
                    unit={unit}
                    value={critical}
                    onValueChange={setCritical}
                    isInvalid={!isCriticalValid}
                  />
                  {kind === 'logs' ? (
                    <SelectField
                      label="Within"
                      value={logWithin.toString()}
                      onValueChange={(value) => setLogWithin(Number(value))}
                      items={LOG_WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
                    />
                  ) : (
                    <UnitField
                      label={`Warning ${direction}`}
                      unit={unit}
                      value={warning}
                      onValueChange={setWarning}
                      placeholder="Off"
                      isInvalid={!isWarningValid}
                    />
                  )}
                </FieldGrid>
              </div>
            </FormSection>
          )}

          <FormSection title="Timing" description="When it fires, when it calms down, and how often it nudges.">
            <SettingRow title="Fire After" description="How long it must stay over a limit before anyone hears">
              {(labelId) => (
                <PlainSelect
                  labelId={labelId}
                  value={forSecs.toString()}
                  onValueChange={(value) => setForSecs(Number(value))}
                  items={FOR_OPTIONS.map((option) => ({
                    value: option.value.toString(),
                    label: option.value === 0 ? 'Immediately' : `Breached for ${option.label}`,
                  }))}
                />
              )}
            </SettingRow>
            {hasThreshold(kind) && (
              <SettingRow
                title="Recovery Limit"
                description={`Optional. Must drop ${kind === 'cert_days' ? 'above' : 'below'} this to resolve, so it does not flap`}
              >
                {(labelId) => (
                  <UnitInput
                    labelId={labelId}
                    unit={unit}
                    value={recovery}
                    onValueChange={setRecovery}
                    placeholder="Off"
                    isInvalid={!isRecoveryValid}
                  />
                )}
              </SettingRow>
            )}
            <SettingRow title="Resolve After" description="How long it must stay under the limit before it resolves">
              {(labelId) => (
                <PlainSelect
                  labelId={labelId}
                  value={resolveForSecs.toString()}
                  onValueChange={(value) => setResolveForSecs(Number(value))}
                  items={RESOLVE_OPTIONS.map((option) => ({
                    value: option.value.toString(),
                    label: option.label.replace('After ', ''),
                  }))}
                />
              )}
            </SettingRow>
            <SettingRow title="Remind" description="While it is still firing">
              {(labelId) => (
                <PlainSelect labelId={labelId} value={remind} onValueChange={(value) => setRemind(value ?? 'off')} items={REMIND_OPTIONS} />
              )}
            </SettingRow>
          </FormSection>

          <FormSection
            title="Noise Control"
            description="For rules that get loud. The defaults are fine for most."
            action={
              isNoiseOpen && (
                <Button
                  type="button"
                  variant="plain"
                  className="w-fit"
                  onClick={() => {
                    setNoData('keep');
                    setMessageLimit('off');
                    setQuietHours([]);
                    setIsNoiseOpen(false);
                  }}
                >
                  <Icon name="undo" className="text-icon-secondary" />
                  Reset to Defaults
                </Button>
              )
            }
          >
            {!isNoiseOpen && (
              <div className="flex items-center gap-3 px-4 py-3.5">
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-body font-medium">Using the defaults</span>
                  <span className="text-body text-text-secondary">
                    {noun === 'cron' ? '' : 'Keeps the last state when data stops · '}No message limit · No quiet hours
                  </span>
                </div>
                <Button type="button" variant="secondary" className="w-fit" onClick={() => setIsNoiseOpen(true)}>
                  Customize
                </Button>
              </div>
            )}
            {isNoiseOpen && (
              <>
                {noun !== 'cron' && (
                  <SettingRow title="If Data Stops" description="When an agent disconnects or a check is paused">
                    {(labelId) => (
                      <PlainSelect
                        labelId={labelId}
                        value={noData}
                        onValueChange={(value) => setNoData((value ?? 'keep') as NoData)}
                        items={NO_DATA_OPTIONS}
                      />
                    )}
                  </SettingRow>
                )}
                <SettingRow title="Message Limit" description="Past it, one summary goes out and the rule stays quiet for the hour">
                  {(labelId) => (
                    <PlainSelect
                      labelId={labelId}
                      value={messageLimit}
                      onValueChange={(value) => setMessageLimit(value ?? 'off')}
                      items={MESSAGE_LIMIT_OPTIONS}
                    />
                  )}
                </SettingRow>
                <div className="flex flex-col">
                  <SettingRow
                    title="Quiet Hours"
                    description="No messages during planned work, like a nightly backup or a deploy window"
                  >
                    {quietHours.length === 0
                      ? (labelId) => (
                          <PlainSelect
                            labelId={labelId}
                            value="off"
                            onValueChange={(value) => value === 'on' && setQuietHours([DEFAULT_QUIET_WINDOW])}
                            items={[
                              { value: 'off', label: 'Off' },
                              { value: 'on', label: 'Weekdays, 02:00 to 03:00' },
                            ]}
                          />
                        )
                      : undefined}
                  </SettingRow>
                  {quietHours.length > 0 && (
                    <div className="px-4 pb-4">
                      <QuietHoursField
                        windows={quietHours}
                        timeZone={timeZone}
                        onWindowsChange={setQuietHours}
                        onTimeZoneChange={setTimeZone}
                      />
                    </div>
                  )}
                </div>
              </>
            )}
          </FormSection>

          <FormSection title={noun === 'check' ? 'Checks' : noun === 'cron' ? 'Crons' : 'Servers'}>
            <div className="p-4">
              <FieldGrid hint={describeMatches(matched.map((target) => target.name), noun, scopeKind)}>
                <SelectField
                  label="Scope"
                  value={scopeKind}
                  onValueChange={(value) => setScopeKind((value ?? 'all') as ScopeKind)}
                  items={scopeItems}
                />
                {scopeKind === 'label' ? (
                  <SelectField
                    label="Label"
                    placeholder={labels.length > 0 ? 'Pick a label' : 'No server has a label yet'}
                    value={label || null}
                    onValueChange={(value) => setLabel(value ?? '')}
                    items={labels.map((item) => ({ value: item, label: item }))}
                  />
                ) : (
                  <div />
                )}
              </FieldGrid>
            </div>
            {scopeKind === 'picked' && (
              <div className="flex flex-col py-1">
                {targets.length === 0 && <p className="px-4 py-2 text-body text-text-secondary">No {noun}s yet.</p>}
                {targets.map((target) => (
                  <label key={target.id} className="flex cursor-pointer items-center gap-3 px-4 py-2 hover:bg-surface-subtle">
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
            )}
          </FormSection>

          <FormSection
            title="Notify"
            description="Pick which severities each integration gets. Every one also hears when the alert resolves."
            action={
              <Button
                type="button"
                variant="plain"
                className="w-fit"
                disabled={!condition || integrationIds.length === 0 || test.isPending}
                onClick={() =>
                  condition && test.mutate({ name: name.trim() || 'Untitled Rule', condition, for_secs: forSecs, integration_ids: integrationIds })
                }
              >
                {test.isPending ? <Icon name="loader" className="animate-spin" /> : <Icon name="send" />}
                Send Test Alert
              </Button>
            }
          >
            {integrations?.map((integration) => (
              <NotifyRow
                key={integration.id}
                integration={integration}
                value={levels[integration.id] ?? 'off'}
                onValueChange={(value) => setNotifyLevels({ ...levels, [integration.id]: value })}
              />
            ))}
            <button
              type="button"
              onClick={() => setIsAddOpen(true)}
              className="flex items-center gap-2.5 bg-surface-subtle px-4 py-2.5 text-left text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
            >
              <Icon name="plus" />
              Add Integration
            </button>
          </FormSection>

          <FormSection title="Incident" description="Open an incident for bad outages, so the team works from one timeline.">
            <label className="flex items-center gap-3 px-4 py-3">
              <Icon name="flag" />
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span id={declareIncidentId} className="text-body font-medium">
                  Declare an incident when this fires
                </span>
                <span className="text-body text-text-secondary">
                  Daroyan declares it, links this alert and adds what it fires on as an affected component.
                </span>
              </span>
              <Switch.Root
                aria-labelledby={declareIncidentId}
                checked={declareIncident}
                onCheckedChange={setDeclareIncident}
              >
                <Switch.Thumb />
              </Switch.Root>
            </label>
            {declareIncident && (
              <div className="flex flex-col gap-5 p-4">
                <FieldGrid hint="Severity sets the colour everywhere: Minor grey, Major amber, Critical red.">
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
              </div>
            )}
          </FormSection>
        </form>

        <aside aria-label="Preview" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <div className="flex flex-col gap-2 border-b border-border-row pb-4">
            <div className="flex items-center justify-between">
              <h2 className="text-body font-medium">Preview</h2>
              <span className="text-body text-text-tertiary">Last {backtest?.days ?? 7} days</span>
            </div>
            <BacktestBox backtest={condition && scope ? backtest : undefined} />
          </div>
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
        </aside>
      </div>

      <IntegrationDialog
        open={isAddOpen}
        onOpenChange={setIsAddOpen}
        onSaved={(saved) => setNotifyLevels({ ...levels, [saved.id]: 'both' })}
      />
    </div>
  );
}

const DECLARE_SEVERITY_OPTIONS = [
  { value: 'minor', label: 'Minor' },
  { value: 'major', label: 'Major' },
  { value: 'critical', label: 'Critical' },
];

const DECLARE_SHOWN_OPTIONS = [
  { value: 'shown', label: 'Yes, on pages that include it' },
  { value: 'hidden', label: 'No, internal until someone posts' },
];

function describeMatches(names: string[], noun: string, scopeKind: ScopeKind) {
  const later =
    scopeKind === 'picked' ? '' : ` ${noun[0]!.toUpperCase()}${noun.slice(1)}s added later join the rule when they match.`;
  if (names.length === 0) {
    return `Matches no ${noun}s yet.${later}`;
  }
  const count = names.length === 1 ? `1 ${noun}` : `${names.length} ${noun}s`;
  return `Matches ${count}: ${joinNames(names)}.${later}`;
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
      <div className={cn('grid grid-cols-1 gap-3', columns === 3 ? 'sm:grid-cols-3' : 'sm:grid-cols-2')}>{children}</div>
      {hint && <p className="text-caption text-text-secondary">{hint}</p>}
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
  labelId?: string;
};

function UnitInput(props: UnitInputProps) {
  const { id, labelId, unit, value, onValueChange, placeholder, isInvalid } = props;

  return (
    <div className="relative">
      <Input
        id={id}
        aria-labelledby={labelId}
        inputMode="decimal"
        value={value}
        onValueChange={(next) => onValueChange(next.replace(/[^0-9.]/g, ''))}
        placeholder={placeholder}
        aria-invalid={isInvalid || undefined}
        className={cn(unit && 'pr-12')}
      />
      {unit && (
        <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-body text-text-tertiary">
          {unit}
        </span>
      )}
    </div>
  );
}

type SettingRowProps = {
  title: string;
  description: string;
  children?: (labelId: string) => ReactNode;
};

function SettingRow(props: SettingRowProps) {
  const { title, description, children } = props;

  const labelId = useId();

  return (
    <div className="flex items-center gap-3 px-4 py-3.5">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={labelId} className="text-body font-medium">
          {title}
        </span>
        <span className="text-body text-text-secondary">{description}</span>
      </div>
      {children && <div className="w-60 shrink-0">{children(labelId)}</div>}
    </div>
  );
}

type PlainSelectProps = {
  labelId: string;
  value: string;
  onValueChange: (value: string | null) => void;
  items: SelectItemData[];
};

function PlainSelect(props: PlainSelectProps) {
  const { labelId, value, onValueChange, items } = props;

  return (
    <Select.Root items={items} value={value} onValueChange={(next) => onValueChange(next as string | null)}>
      <Select.Trigger aria-labelledby={labelId}>
        <Select.Value />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner>
          <Select.Popup>
            {items.map((item) => (
              <Select.Item key={item.value} value={item.value}>
                {item.label}
              </Select.Item>
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
      <div className="flex items-end gap-3">
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
    <div className="flex items-center gap-3 px-4 py-3">
      <Icon name={icon} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={titleId} className="truncate text-body font-medium">
          {integration.name}
        </span>
        <span className="truncate text-body text-text-secondary">
          {getIntegrationLabel(integration.kind)} · {integration.target}
        </span>
      </div>
      <div className="w-48 shrink-0">
        <Select.Root items={NOTIFY_OPTIONS} value={value} onValueChange={(next) => onValueChange(next as NotifyLevel)}>
          <Select.Trigger aria-labelledby={titleId}>
            <Select.Value />
          </Select.Trigger>
          <Select.Portal>
            <Select.Positioner>
              <Select.Popup>
                {NOTIFY_OPTIONS.map((option) => (
                  <Select.Item key={option.value} value={option.value}>
                    {option.label}
                  </Select.Item>
                ))}
              </Select.Popup>
            </Select.Positioner>
          </Select.Portal>
        </Select.Root>
      </div>
    </div>
  );
}

type BacktestBoxProps = {
  backtest?: Schemas['AlertRuleBacktest'];
};

function BacktestBox(props: BacktestBoxProps) {
  const { backtest } = props;

  const count = backtest?.hits.length ?? 0;

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
