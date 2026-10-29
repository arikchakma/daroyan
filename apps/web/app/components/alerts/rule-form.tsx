import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { getBrowserTimeZone } from '~/components/alerts/quiet-hours-field';
import { RuleConditionSection } from '~/components/alerts/rule-condition-section';
import { RuleMoreOptions } from '~/components/alerts/rule-more-options';
import { RuleNotifySection } from '~/components/alerts/rule-notify-section';
import { RulePreview } from '~/components/alerts/rule-preview';
import { RuleScopeSection } from '~/components/alerts/rule-scope-section';
import { RuleStatusPanel } from '~/components/alerts/rule-status-panel';
import { RuleTemplates } from '~/components/alerts/rule-templates';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { createAlertRuleOptions, listAlertsOptions, listIntegrationsOptions, updateAlertRuleOptions } from '~/queries/alerts';
import { listStatusPagesOptions } from '~/queries/status-pages';
import type { NotifyLevel } from '~/utils/alerts';
import { formatDate } from '~/utils/format';
import {
  applyTemplate,
  getTimesText,
  isOptionalNumber,
  readCondition,
  readInitialNotifyLevels,
  readInitialRuleForm,
  readRuleOptions,
  readScope,
} from '~/utils/rule-form';
import type { RuleDraft, RuleFormState } from '~/utils/rule-form';

const RECENT_MS = 30 * 86_400_000;

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

function RuleFormBody(props: RuleFormBodyProps) {
  const { rule, draft, onDiscard } = props;

  const formRef = useRef<HTMLFormElement>(null);
  const navigate = useNavigate();

  const { data: integrations } = useQuery(listIntegrationsOptions());
  const { data: history = [] } = useQuery({ ...listAlertsOptions('all', rule?.id), enabled: !!rule });
  const { data: statusPages = [] } = useQuery(listStatusPagesOptions());

  const [form, setForm] = useState(() => readInitialRuleForm({ rule, draft, timeZone: getBrowserTimeZone() }));
  const [notifyLevels, setNotifyLevels] = useState(() => readInitialNotifyLevels(rule));

  const onChange = (patch: Partial<RuleFormState>) => setForm((current) => ({ ...current, ...patch }));

  const levels: Record<string, NotifyLevel> =
    notifyLevels ?? Object.fromEntries((integrations ?? []).map((integration) => [integration.id, 'both']));
  const integrationIds = Object.keys(levels).filter((id) => levels[id] !== 'off');
  const condition = readCondition(form);
  const scope = readScope(form);
  const options = readRuleOptions({
    form,
    warningIntegrationIds: Object.keys(levels).filter((id) => levels[id] === 'both'),
    severityLabel: rule?.options.severity_label ?? null,
  });
  const declare = {
    declare_incident: form.declareIncident,
    declare_severity: form.declareIncident ? form.declareSeverity : null,
    declare_hidden_page_ids: form.declareIncident && form.declareShown === 'hidden' ? statusPages.map((page) => page.id) : [],
  };

  const onSaved = (saved: Schemas['AlertRule']) => {
    queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
    toast.success(rule ? `Saved ${saved.name}` : `Created ${saved.name}`);
    navigate(`/alerts/${saved.id}`);
  };

  const create = useMutation({ ...createAlertRuleOptions(), onSuccess: onSaved, meta: { errorMessage: 'The rule could not be created' } });
  const update = useMutation({ ...updateAlertRuleOptions(), onSuccess: onSaved, meta: { errorMessage: 'The rule could not be saved' } });

  const name = form.name.trim();
  const canSave = !!name && !!condition && !!scope && isOptionalNumber(form.warning) && isOptionalNumber(form.recovery);
  const snapshot = JSON.stringify([
    name,
    condition,
    form.forSecs,
    scope,
    integrationIds,
    options,
    form.declareIncident,
    form.declareSeverity,
    form.declareShown,
  ]);
  const [savedSnapshot] = useState(snapshot);
  const isSaving = create.isPending || update.isPending;
  const backTo = rule ? `/alerts/${rule.id}` : '/alerts';
  const now = Date.now();
  const firedCount = history.filter((alert) => now - new Date(alert.started_at).getTime() < RECENT_MS).length;

  const save = () => {
    if (!condition || !scope) {
      return;
    }

    const body = { name, condition, for_secs: form.forSecs, scope, integration_ids: integrationIds, options, ...declare };

    if (rule) {
      update.mutate({ ruleId: rule.id, body });
    } else {
      create.mutate(body);
    }
  };

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
                ? `Created ${formatDate(rule.created_at)}. Fired ${getTimesText(firedCount)} in the last 30 days.`
                : 'Pick a condition and a limit. Everything else starts on a sensible default.'}
            </p>
          </div>
          <RuleConditionSection form={form} onChange={onChange} isNew={!rule} />
          <RuleScopeSection form={form} onChange={onChange} />
          <RuleNotifySection
            levels={levels}
            onLevelsChange={setNotifyLevels}
            testBody={condition && { name: name || 'Untitled Rule', condition, for_secs: form.forSecs, integration_ids: integrationIds }}
          />
          <RuleMoreOptions form={form} onChange={onChange} isEditing={!!rule} />
        </form>

        <aside aria-label="Preview" className="flex w-65 shrink-0 flex-col gap-5 pt-2 max-lg:hidden">
          <RulePreview rule={rule} condition={condition} scope={scope} forSecs={form.forSecs} />
          {rule ? (
            <RuleStatusPanel rule={rule} />
          ) : (
            <RuleTemplates onApply={(template) => onChange(applyTemplate(form, template))} />
          )}
        </aside>
      </div>
    </div>
  );
}
