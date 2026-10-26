import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useId, useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { FormSection, SelectField } from '~/components/alerts/rule-form';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { Switch } from '~/components/ui/switch';
import { CheckRules, getCheckRules } from '~/components/uptime/check-rules';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { createAlertRuleOptions, listAlertRulesOptions, listIntegrationsOptions } from '~/queries/alerts';
import { createUptimeCheckOptions, updateUptimeCheckOptions } from '~/queries/uptime';
import type { UptimeCheckInput } from '~/queries/uptime';
import { joinNames } from '~/utils/alerts';
import { INTERVAL_OPTIONS, TIMEOUT_OPTIONS } from '~/utils/uptime';

type CheckKind = 'http' | 'tcp';

const KINDS: { value: CheckKind; label: string }[] = [
  { value: 'http', label: 'HTTP' },
  { value: 'tcp', label: 'TCP Port' },
];

const METHOD_OPTIONS = ['GET', 'HEAD', 'POST'].map((method) => ({ value: method, label: method }));

type CheckFormProps = {
  check?: Schemas['UptimeCheck'];
};

export function CheckForm(props: CheckFormProps) {
  const { check } = props;

  const [version, setVersion] = useState(0);

  return <CheckFormBody key={version} check={check} onDiscard={() => setVersion(version + 1)} />;
}

type CheckFormBodyProps = CheckFormProps & {
  onDiscard: () => void;
};

function readOptionalNumber(value: string) {
  return value.trim() === '' ? null : Number(value);
}

function CheckFormBody(props: CheckFormBodyProps) {
  const { check, onDiscard } = props;

  const formRef = useRef<HTMLFormElement>(null);
  const navigate = useNavigate();

  const [name, setName] = useState(check?.name ?? '');
  const [kind, setKind] = useState<CheckKind>(check?.kind === 'tcp' ? 'tcp' : 'http');
  const [target, setTarget] = useState(check?.target ?? '');
  const [method, setMethod] = useState(check?.method ?? 'GET');
  const [expectedStatus, setExpectedStatus] = useState(check?.expected_status?.toString() ?? '');
  const [bodyContains, setBodyContains] = useState(check?.body_contains ?? '');
  const [intervalSecs, setIntervalSecs] = useState(check?.interval_secs ?? 60);
  const [timeoutSecs, setTimeoutSecs] = useState(check?.timeout_secs ?? 10);

  const status = readOptionalNumber(expectedStatus);
  const isStatusValid = status === null || (Number.isInteger(status) && status >= 100 && status <= 599);
  const isTargetValid =
    kind === 'http' ? /^https?:\/\/\S+$/.test(target.trim()) : /^[^\s:]+:\d{1,5}$/.test(target.trim());

  const body: UptimeCheckInput = {
    name: name.trim(),
    kind,
    target: target.trim(),
    method,
    expected_status: kind === 'http' ? status : null,
    body_contains: kind === 'http' ? bodyContains.trim() || null : null,
    interval_secs: intervalSecs,
    timeout_secs: Math.min(timeoutSecs, intervalSecs - 1),
    paused: check?.paused ?? false,
  };

  const { data: rules } = useQuery(listAlertRulesOptions());
  const { data: integrations } = useQuery(listIntegrationsOptions());
  const isCovered = getCheckRules(rules ?? []).some((rule) => rule.enabled && rule.condition.kind === 'uptime_down');
  const [pickedAlertDown, setPickedAlertDown] = useState<boolean>();
  const alertDown = pickedAlertDown ?? (!!rules && !isCovered);
  const createRule = useMutation({ ...createAlertRuleOptions(), meta: { errorMessage: 'The check was created, but its alert rule was not' } });

  const onSaved = async (saved: Schemas['UptimeCheck']) => {
    if (!check && alertDown) {
      await createRule
        .mutateAsync({
          name: `${saved.name} Down`,
          condition: { kind: 'uptime_down' },
          for_secs: 0,
          scope: { kind: 'checks', check_ids: [saved.id] },
          integration_ids: (integrations ?? []).map((integration) => integration.id),
        })
        .then(() => queryClient.invalidateQueries({ queryKey: ['alert-rules'] }))
        .catch(() => undefined);
    }
    queryClient.invalidateQueries({ queryKey: ['uptime-checks'] });
    toast.success(check ? `Saved ${saved.name}` : `Created ${saved.name}`);
    navigate(`/uptime/${saved.id}`);
  };
  const create = useMutation({
    ...createUptimeCheckOptions(),
    onSuccess: onSaved,
    meta: { errorMessage: 'The check could not be created' },
  });
  const update = useMutation({
    ...updateUptimeCheckOptions(),
    onSuccess: onSaved,
    meta: { errorMessage: 'The check could not be saved' },
  });

  const canSave = !!body.name && isTargetValid && isStatusValid;
  const snapshot = JSON.stringify(body);
  const [savedSnapshot] = useState(snapshot);
  const isSaving = create.isPending || update.isPending || createRule.isPending;
  const backTo = check ? `/uptime/${check.id}` : '/uptime';

  const save = () => {
    if (check) {
      update.mutate({ checkId: check.id, body });
    } else {
      create.mutate(body);
    }
  };

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo={backTo} icon="activity" title="Uptime">
        {!check && (
          <>
            <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={backTo} />}>
              Cancel
            </Button>
            <Button className="w-fit" disabled={!canSave || isSaving} onClick={() => formRef.current?.requestSubmit()}>
              {isSaving && <Icon name="loader" className="animate-spin" />}
              Create Check
            </Button>
          </>
        )}
      </PageBar>
      {check && (
        <SaveBar
          isDirty={snapshot !== savedSnapshot}
          isSaving={isSaving}
          canSave={canSave}
          onSave={() => formRef.current?.requestSubmit()}
          onDiscard={onDiscard}
        />
      )}

      <div className="flex justify-center px-10 pt-3 pb-16 max-lg:px-4">
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
            <h1 className="text-title-page font-semibold">{check ? 'Edit Check' : 'New Check'}</h1>
            <p className="text-body text-text-secondary">
              Daroyan calls it from this server on a schedule, and tells you when it stops answering.
            </p>
          </div>

          <FormSection title="Check">
            <div className="flex flex-col gap-5 p-4">
              <FieldRow label="Name" hint="Shown in the list and in alert messages.">
                {({ inputId, hintId }) => (
                  <Input
                    id={inputId}
                    aria-describedby={hintId}
                    value={name}
                    onValueChange={setName}
                    placeholder="API"
                    maxLength={100}
                    required
                    autoFocus={!check}
                  />
                )}
              </FieldRow>
              <div className="flex flex-wrap items-center gap-4">
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-body font-medium">Type</span>
                  <span className="text-body text-text-secondary">
                    {kind === 'http' ? 'Sends a request and reads the response.' : 'Opens a connection to the port.'}
                  </span>
                </div>
                <div role="radiogroup" aria-label="Type" className="flex items-center gap-1">
                  {KINDS.map((item) => (
                    <button
                      key={item.value}
                      type="button"
                      role="radio"
                      aria-checked={kind === item.value}
                      onClick={() => setKind(item.value)}
                      className="h-7 rounded-md px-2.5 text-body text-text-secondary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 aria-checked:bg-surface-fill aria-checked:font-medium aria-checked:text-text-primary"
                    >
                      {item.label}
                    </button>
                  ))}
                </div>
              </div>
              {kind === 'http' ? (
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-[7rem_1fr]">
                  <SelectField label="Method" value={method} onValueChange={(value) => setMethod(value ?? 'GET')} items={METHOD_OPTIONS} />
                  <FieldRow label="URL">
                    {({ inputId }) => (
                      <Input
                        id={inputId}
                        value={target}
                        onValueChange={setTarget}
                        placeholder="https://api.example.com/health"
                        aria-invalid={(!!target && !isTargetValid) || undefined}
                        className="font-mono"
                        required
                      />
                    )}
                  </FieldRow>
                </div>
              ) : (
                <FieldRow label="Host and Port">
                  {({ inputId }) => (
                    <Input
                      id={inputId}
                      value={target}
                      onValueChange={setTarget}
                      placeholder="smtp.example.com:587"
                      aria-invalid={(!!target && !isTargetValid) || undefined}
                      className="font-mono"
                      required
                    />
                  )}
                </FieldRow>
              )}
            </div>
          </FormSection>

          {kind === 'http' && (
            <FormSection title="Expect" description="The check fails when the response does not match.">
              <div className="grid grid-cols-1 gap-3 p-4 sm:grid-cols-2">
                <FieldRow label="Status">
                  {({ inputId }) => (
                    <Input
                      id={inputId}
                      inputMode="numeric"
                      value={expectedStatus}
                      onValueChange={(next) => setExpectedStatus(next.replace(/\D/g, ''))}
                      placeholder="Any 2xx or 3xx"
                      aria-invalid={!isStatusValid || undefined}
                    />
                  )}
                </FieldRow>
                <FieldRow label="Body Contains">
                  {({ inputId }) => (
                    <Input
                      id={inputId}
                      value={bodyContains}
                      onValueChange={setBodyContains}
                      placeholder="Any body"
                      maxLength={200}
                    />
                  )}
                </FieldRow>
              </div>
            </FormSection>
          )}

          <FormSection
            title="Schedule"
            description="Runs from this server. The timeout must be shorter than the interval."
          >
            <div className="grid grid-cols-1 gap-3 p-4 sm:grid-cols-2">
              <SelectField
                label="Every"
                value={intervalSecs.toString()}
                onValueChange={(value) => setIntervalSecs(Number(value))}
                items={INTERVAL_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
              />
              <SelectField
                label="Timeout"
                value={timeoutSecs.toString()}
                onValueChange={(value) => setTimeoutSecs(Number(value))}
                items={TIMEOUT_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
              />
            </div>
          </FormSection>

          <FormSection
            title="Alerts"
            description="Alert rules decide who hears about this check. A check is down after 2 failures in a row."
          >
            {!check && (
              <AlertDownRow
                name={name.trim() || 'Check'}
                integrations={integrations ?? []}
                checked={alertDown}
                onCheckedChange={setPickedAlertDown}
              />
            )}
            <CheckRules checkId={check?.id} checkName={check?.name} />
          </FormSection>
        </form>
      </div>

    </div>
  );
}

type AlertDownRowProps = {
  name: string;
  integrations: Schemas['Integration'][];
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
};

function AlertDownRow(props: AlertDownRowProps) {
  const { name, integrations, checked, onCheckedChange } = props;

  const titleId = useId();
  const names = integrations.map((integration) => integration.name);
  const audience =
    names.length === 0
      ? 'No integrations yet, so add one in Settings to hear about it.'
      : `Sent to ${names.length > 3 ? `${names.length} integrations` : joinNames(names)}.`;

  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <Icon name="bell" className="shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={titleId} className="truncate text-body font-medium">
          Alert when it goes down
        </span>
        <span className="text-body text-text-secondary">
          Creates the rule {name} Down for this check. {audience}
        </span>
      </div>
      <Switch.Root aria-labelledby={titleId} checked={checked} onCheckedChange={onCheckedChange}>
        <Switch.Thumb />
      </Switch.Root>
    </div>
  );
}
