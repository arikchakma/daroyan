import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';

import { FormSection } from '~/components/alerts/form-section';
import { PickedTargets } from '~/components/alerts/picked-targets';
import type { Target } from '~/components/alerts/picked-targets';
import { SelectField } from '~/components/alerts/select-field';
import { SettingRow } from '~/components/alerts/setting-row';
import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import { listServersOptions } from '~/queries/servers';
import { listUptimeChecksOptions } from '~/queries/uptime';
import { getMatchesText, readTargetNoun, readTargetNouns } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState, ScopeKind, TargetNoun } from '~/utils/rule-form';

type RuleScopeSectionProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
};

export function RuleScopeSection(props: RuleScopeSectionProps) {
  const { form, onChange } = props;

  const { data: servers } = useQuery(listServersOptions());
  const { data: checks } = useQuery(listUptimeChecksOptions());
  const { data: crons } = useQuery({ queryKey: ['crons'], queryFn: () => request(apiClient.GET('/v1/crons')) });

  const { scopeKind } = form;
  const noun = readTargetNoun(form.kind);
  const nouns = readTargetNouns(noun);

  const labels = [...new Set((servers ?? []).flatMap((server) => server.labels))].sort();
  const targets = getTargets({ noun, servers, checks, crons });
  const matches = getMatchesText(
    targets.filter((target) => scopeKind === 'all' || target.labels.includes(form.label)).map((target) => target.name),
    noun
  );

  const scopeItems: { value: ScopeKind; label: string }[] = [
    { value: 'all', label: `All ${nouns}` },
    ...(noun === 'server' ? [{ value: 'label' as const, label: 'With Label' }] : []),
    { value: 'picked', label: `Picked ${nouns}` },
  ];

  return (
    <FormSection title={nouns}>
      <SettingRow
        title="Scope"
        isCaption
        description={scopeKind === 'picked' ? `Only the ${noun}s you pick.` : `${nouns} added later join the rule when they match.`}
        actions={
          <div role="radiogroup" aria-label="Scope" className="flex shrink-0 items-center gap-0.5">
            {scopeItems.map((item) => (
              <button
                key={item.value}
                type="button"
                role="radio"
                aria-checked={scopeKind === item.value}
                onClick={() => onChange({ scopeKind: item.value })}
                className="h-7 rounded-sm px-2.5 text-body font-medium whitespace-nowrap text-text-secondary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 aria-checked:bg-surface-fill aria-checked:text-text-primary"
              >
                {item.label}
              </button>
            ))}
          </div>
        }
      />
      {scopeKind === 'picked' && (
        <PickedTargets
          noun={noun}
          targets={targets}
          pickedIds={form.pickedIds}
          onPickedIdsChange={(pickedIds) => onChange({ pickedIds })}
        />
      )}
      {scopeKind === 'label' && (
        <div className="p-3.5">
          <SelectField
            label="Label"
            placeholder={labels.length > 0 ? 'Pick a label' : 'No server has a label yet'}
            value={form.label || null}
            onValueChange={(value) => onChange({ label: value ?? '' })}
            items={labels.map((item) => ({ value: item, label: item }))}
            hint={matches}
          />
        </div>
      )}
      {scopeKind === 'all' && <p className="p-3.5 text-caption leading-4.5 text-text-secondary">{matches}</p>}
    </FormSection>
  );
}

type GetTargetsOptions = {
  noun: TargetNoun;
  servers?: Schemas['ServerItem'][];
  checks?: Schemas['UptimeCheck'][];
  crons?: Schemas['Cron'][];
};

function getTargets(options: GetTargetsOptions): Target[] {
  const { noun, servers = [], checks = [], crons = [] } = options;

  if (noun === 'check') {
    return checks.map((check) => ({ id: check.id, name: check.name, detail: check.target, labels: [] }));
  }
  if (noun === 'cron') {
    return crons.map((cron) => ({ id: cron.id, name: cron.name, detail: cron.expression ?? '', labels: [] }));
  }

  return servers.map((server) => ({ id: server.id, name: server.name, detail: server.labels.join(', '), labels: server.labels }));
}
