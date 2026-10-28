import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useId, useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { getTimeZones } from '~/components/alerts/quiet-hours-field';
import { FormSection } from '~/components/alerts/form-section';
import { SelectField } from '~/components/alerts/select-field';
import { CronRules } from '~/components/crons/cron-rules';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Chip } from '~/components/ui/chip';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { Switch } from '~/components/ui/switch';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { createAlertRuleOptions, listAlertRulesOptions, listIntegrationsOptions } from '~/queries/alerts';
import { createCronOptions, cronPreviewOptions, updateCronOptions } from '~/queries/crons';
import type { CronInput } from '~/queries/crons';
import { listServersOptions } from '~/queries/servers';
import { joinNames } from '~/utils/alerts';
import type { ConditionKind } from '~/utils/alerts';
import {
  COMMON_SCHEDULES,
  GRACE_OPTIONS,
  getPingUrl,
  INTERVAL_OPTIONS,
  joinTimes,
  MAX_DURATION_OPTIONS,
} from '~/utils/crons';

type ScheduleKind = 'cron' | 'every';

const SCHEDULE_OPTIONS: { value: ScheduleKind; label: string }[] = [
  { value: 'cron', label: 'Cron expression' },
  { value: 'every', label: 'Every' },
];

const NO_SERVER = 'none';

function getZoneItems() {
  return [{ value: 'UTC', label: 'UTC' }, ...getTimeZones().filter((zone) => zone.value !== 'UTC')];
}

function toItems(options: { value: number; label: string }[]) {
  return options.map((option) => ({ value: option.value.toString(), label: option.label }));
}

type CronFormProps = {
  cron?: Schemas['Cron'];
};

export function CronForm(props: CronFormProps) {
  const { cron } = props;

  const [version, setVersion] = useState(0);

  return <CronFormBody key={version} cron={cron} onDiscard={() => setVersion(version + 1)} />;
}

type CronFormBodyProps = CronFormProps & {
  onDiscard: () => void;
};

function CronFormBody(props: CronFormBodyProps) {
  const { cron, onDiscard } = props;

  const formRef = useRef<HTMLFormElement>(null);
  const navigate = useNavigate();

  const [name, setName] = useState(cron?.name ?? '');
  const [scheduleKind, setScheduleKind] = useState<ScheduleKind>(cron?.schedule_kind === 'every' ? 'every' : 'cron');
  const [expression, setExpression] = useState(cron?.expression ?? '*/5 * * * *');
  const [intervalSecs, setIntervalSecs] = useState(cron?.interval_secs ?? 3600);
  const [timeZone, setTimeZone] = useState(cron?.time_zone ?? 'UTC');
  const [graceSecs, setGraceSecs] = useState(cron?.grace_secs ?? 120);
  const [maxDurationSecs, setMaxDurationSecs] = useState(cron?.max_duration_secs ?? 0);
  const [serverId, setServerId] = useState(cron?.server_id ?? NO_SERVER);
  const [command, setCommand] = useState(cron?.command ?? '');

  const { data: servers } = useQuery(listServersOptions());
  const preview = useQuery(cronPreviewOptions(scheduleKind === 'cron' ? expression : '', timeZone));
  const isExpressionValid = scheduleKind === 'every' || (!!expression.trim() && !preview.isError);

  const body: CronInput = {
    name: name.trim(),
    schedule_kind: scheduleKind,
    expression: scheduleKind === 'cron' ? expression.trim() : null,
    interval_secs: scheduleKind === 'every' ? intervalSecs : null,
    time_zone: timeZone,
    grace_secs: graceSecs,
    max_duration_secs: maxDurationSecs || null,
    server_id: serverId === NO_SERVER ? null : serverId,
    command: command.trim() || null,
    paused: cron?.paused ?? false,
  };

  const { data: rules } = useQuery(listAlertRulesOptions());
  const { data: integrations } = useQuery(listIntegrationsOptions());
  const [alertMissed, setAlertMissed] = useState(true);
  const [alertFailed, setAlertFailed] = useState(true);
  const createRule = useMutation({
    ...createAlertRuleOptions(),
    meta: { errorMessage: 'The cron was created, but its alert rules were not' },
  });

  const onSaved = async (saved: Schemas['Cron']) => {
    if (!cron) {
      const picked: [boolean, ConditionKind, string][] = [
        [alertMissed, 'cron_missed', 'Missed'],
        [alertFailed, 'cron_failed', 'Failed'],
      ];
      for (const [isPicked, kind, label] of picked) {
        if (!isPicked) {
          continue;
        }
        await createRule
          .mutateAsync({
            name: `${saved.name} ${label}`,
            condition: { kind } as Schemas['Condition'],
            for_secs: 0,
            scope: { kind: 'crons', cron_ids: [saved.id] },
            integration_ids: (integrations ?? []).map((integration) => integration.id),
          })
          .catch(() => undefined);
      }
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
    }
    queryClient.invalidateQueries({ queryKey: ['crons'] });
    toast.success(cron ? `Saved ${saved.name}` : `Created ${saved.name}`);
    navigate(`/crons/${saved.id}`);
  };
  const create = useMutation({
    ...createCronOptions(),
    onSuccess: onSaved,
    meta: { errorMessage: 'The cron could not be created' },
  });
  const update = useMutation({
    ...updateCronOptions(),
    onSuccess: onSaved,
    meta: { errorMessage: 'The cron could not be saved' },
  });

  const canSave = !!body.name && isExpressionValid;
  const snapshot = JSON.stringify(body);
  const [savedSnapshot] = useState(snapshot);
  const isSaving = create.isPending || update.isPending || createRule.isPending;
  const backTo = cron ? `/crons/${cron.id}` : '/crons';
  const jobName = name.trim() || 'Cron';
  const common = COMMON_SCHEDULES.find((schedule) => schedule.expression === expression.trim());
  const nextRuns = preview.data?.next_runs ?? [];

  const save = () => {
    if (cron) {
      update.mutate({ cronId: cron.id, body });
    } else {
      create.mutate(body);
    }
  };

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo={backTo} icon="clock" title="Crons">
        {!cron && (
          <>
            <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={backTo} />}>
              Cancel
            </Button>
            <Button className="w-fit" disabled={!canSave || isSaving} onClick={() => formRef.current?.requestSubmit()}>
              {isSaving && <Icon name="loader" className="animate-spin" />}
              Create Cron
            </Button>
          </>
        )}
      </PageBar>
      {cron && (
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
            <h1 className="text-title-page font-semibold">{cron ? 'Edit Cron' : 'New Cron'}</h1>
            <p className="text-body text-text-secondary">
              Your job pings Daroyan each time it runs. Daroyan tells you when a ping is late, missing or reports a
              failure.
            </p>
          </div>

          <FormSection title="Job">
            <div className="flex flex-col gap-5 p-4">
              <FieldRow label="Name" hint="Use the name your code or crontab uses, so it is easy to find.">
                {({ inputId, hintId }) => (
                  <Input
                    id={inputId}
                    aria-describedby={hintId}
                    value={name}
                    onValueChange={setName}
                    placeholder="contacts.sync"
                    maxLength={100}
                    required
                    autoFocus={!cron}
                  />
                )}
              </FieldRow>
              <div className="flex flex-col gap-1.5">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <SelectField
                    label="Schedule"
                    value={scheduleKind}
                    onValueChange={(value) => setScheduleKind(value === 'every' ? 'every' : 'cron')}
                    items={SCHEDULE_OPTIONS}
                  />
                  {scheduleKind === 'cron' ? (
                    <SelectField
                      label="Timezone"
                      value={timeZone}
                      onValueChange={(value) => setTimeZone(value ?? 'UTC')}
                      items={getZoneItems()}
                    />
                  ) : (
                    <SelectField
                      label="Every"
                      value={intervalSecs.toString()}
                      onValueChange={(value) => setIntervalSecs(Number(value))}
                      items={toItems(INTERVAL_OPTIONS)}
                    />
                  )}
                </div>
                <p className="text-caption text-text-secondary">
                  {scheduleKind === 'cron'
                    ? 'Paste the line from your crontab, or pick Every for a fixed interval.'
                    : 'Expects a ping this often, counted from the last one.'}
                </p>
              </div>
              {scheduleKind === 'cron' && (
                <FieldRow label="Expression">
                  {({ inputId }) => (
                    <>
                      <Input
                        id={inputId}
                        value={expression}
                        onValueChange={setExpression}
                        placeholder="*/5 * * * *"
                        aria-invalid={!isExpressionValid || undefined}
                        className="font-mono"
                        required
                      />
                      <p className="text-caption text-text-secondary">
                        {!isExpressionValid
                          ? 'Enter 5 fields, like */5 * * * * for every 5 minutes.'
                          : nextRuns.length > 0
                            ? `${common ? `${common.label}. ` : ''}Next runs at ${joinTimes(nextRuns)}.`
                            : ' '}
                      </p>
                    </>
                  )}
                </FieldRow>
              )}
            </div>
          </FormSection>

          <FormSection title="Timing" description="How long Daroyan waits before it calls a run late or failed.">
            <TimingRow
              title="Grace Period"
              description="How late a ping can arrive before the run counts as missed"
              value={graceSecs}
              onValueChange={setGraceSecs}
              options={GRACE_OPTIONS}
            />
            <TimingRow
              title="Max Duration"
              description="A run still going after this is marked failed. Needs a /start ping."
              value={maxDurationSecs}
              onValueChange={setMaxDurationSecs}
              options={MAX_DURATION_OPTIONS}
            />
          </FormSection>

          <FormSection title="Server">
            <div className="flex flex-col gap-1.5 p-4">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <SelectField
                  label="Server"
                  value={serverId}
                  onValueChange={(value) => setServerId(value ?? NO_SERVER)}
                  items={[
                    { value: NO_SERVER, label: 'None' },
                    ...(servers ?? []).map((server) => ({ value: server.id, label: server.name })),
                  ]}
                />
                <FieldRow label="Command">
                  {({ inputId }) => (
                    <Input
                      id={inputId}
                      value={command}
                      onValueChange={setCommand}
                      placeholder="php artisan contacts:sync"
                      maxLength={500}
                      className="font-mono"
                    />
                  )}
                </FieldRow>
              </div>
              <p className="text-caption text-text-secondary">
                Optional. Shows the job next to the server, its logs and metrics.
              </p>
            </div>
          </FormSection>

          <FormSection
            title="Alerts"
            description={
              cron
                ? 'Alert rules decide who hears about this cron.'
                : 'Daroyan creates these rules when you save. Change who hears them in Alerts.'
            }
            action={
              <Button nativeButton={false} variant="plain" className="w-fit" render={<Link to="/alerts" />}>
                View Rules
              </Button>
            }
          >
            {cron ? (
              <CronRules cronId={cron.id} cronName={cron.name} />
            ) : (
              <>
                <AlertRow
                  title="Alert when it misses a run"
                  ruleName={`${jobName} Missed`}
                  integrations={integrations ?? []}
                  checked={alertMissed}
                  onCheckedChange={setAlertMissed}
                />
                <AlertRow
                  title="Alert when it fails"
                  ruleName={`${jobName} Failed`}
                  integrations={integrations ?? []}
                  checked={alertFailed}
                  onCheckedChange={setAlertFailed}
                />
              </>
            )}
          </FormSection>
        </form>

        <aside aria-label="Setup" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <div className="flex flex-col gap-2.5 border-b border-border-row pb-4">
            <div className="flex items-center gap-3">
              <h2 className="flex-1 text-body font-medium">Ping URL</h2>
              {!cron && <span className="text-body text-text-tertiary">After you create it</span>}
            </div>
            <div className="flex flex-col gap-1.5 rounded-xl bg-surface-subtle p-4 text-body text-text-secondary">
              <span className="font-medium break-all text-text-primary">
                curl -fsS {cron ? getPingUrl(cron.ping_key) : `${window.location.host}/p/<key>`}
              </span>
              <span>/start when the job begins</span>
              <span>/fail when it errors</span>
              <span>/&lt;code&gt; to send the exit code</span>
            </div>
          </div>
          {scheduleKind === 'cron' && (
            <div className="flex flex-col gap-2.5">
              <h2 className="text-body font-medium">Common Schedules</h2>
              <div className="flex flex-wrap gap-1.5">
                {COMMON_SCHEDULES.map((schedule) => (
                  <button
                    key={schedule.expression}
                    type="button"
                    onClick={() => setExpression(schedule.expression)}
                    className="rounded-xs outline-none hover:opacity-80 focus-visible:ring-2 focus-visible:ring-text-link/40"
                  >
                    <Chip>{schedule.label}</Chip>
                  </button>
                ))}
              </div>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}

type TimingRowProps = {
  title: string;
  description: string;
  value: number;
  onValueChange: (value: number) => void;
  options: { value: number; label: string }[];
};

function TimingRow(props: TimingRowProps) {
  const { title, description, value, onValueChange, options } = props;

  return (
    <div className="flex flex-wrap items-center gap-4 px-4 py-3">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-body font-medium">{title}</span>
        <span className="text-body text-text-secondary">{description}</span>
      </div>
      <SelectField
        label={title}
        value={value.toString()}
        onValueChange={(next) => onValueChange(Number(next))}
        items={toItems(options)}
        className="w-60 max-sm:w-full [&>span:first-child]:sr-only"
      />
    </div>
  );
}

type AlertRowProps = {
  title: string;
  ruleName: string;
  integrations: Schemas['Integration'][];
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
};

function AlertRow(props: AlertRowProps) {
  const { title, ruleName, integrations, checked, onCheckedChange } = props;

  const titleId = useId();
  const names = integrations.map((integration) => integration.name);
  const audience =
    names.length === 0
      ? 'No integrations yet, so add one in Settings to hear about it.'
      : `sent to ${names.length > 3 ? `${names.length} integrations` : joinNames(names)}.`;

  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <Icon name="bell" className="shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={titleId} className="truncate text-body font-medium">
          {title}
        </span>
        <span className="text-body text-text-secondary">
          Creates {ruleName}
          {names.length === 0 ? `. ${audience}` : `, ${audience}`}
        </span>
      </div>
      <Switch.Root aria-labelledby={titleId} checked={checked} onCheckedChange={onCheckedChange}>
        <Switch.Thumb />
      </Switch.Root>
    </div>
  );
}
