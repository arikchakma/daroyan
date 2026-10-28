import { ConditionFields } from '~/components/alerts/condition-fields';
import { FormSection } from '~/components/alerts/form-section';
import { PlainSelect } from '~/components/alerts/plain-select';
import { SelectField } from '~/components/alerts/select-field';
import { SettingRow } from '~/components/alerts/setting-row';
import { Button } from '~/components/ui/button';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import type { Aggregation, ConditionKind } from '~/utils/alerts';
import { hasAggregation, METRIC_GROUPS } from '~/utils/alerts';
import { CONDITION_LABELS, changeKind, getMeasureGroups, hasWarning } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

type RuleConditionSectionProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
  isNew: boolean;
};

export function RuleConditionSection(props: RuleConditionSectionProps) {
  const { form, onChange, isNew } = props;

  const { kind } = form;
  const measureValue = form.windowSecs === 0 ? 'latest' : `${form.aggregation}:${form.windowSecs}`;

  const changeMeasure = (value: string | null) => {
    if (!value || value === 'latest') {
      onChange({ windowSecs: 0 });
      return;
    }
    const [aggregation, windowSecs] = value.split(':');
    onChange({ aggregation: aggregation as Aggregation, windowSecs: Number(windowSecs) });
  };

  return (
    <FormSection
      title="Rule"
      action={
        hasWarning(kind) && (
          <Button
            type="button"
            variant="tertiary"
            className="w-fit"
            onClick={() =>
              onChange(form.isWarningOpen ? { isWarningOpen: false, warning: '' } : { isWarningOpen: true })
            }
          >
            {form.isWarningOpen ? 'Remove Warning Level' : 'Add Warning Level'}
          </Button>
        )
      }
    >
      <div className="flex flex-col gap-3.5 p-3.5">
        <FieldRow label="Name">
          {({ inputId }) => (
            <Input
              id={inputId}
              value={form.name}
              onValueChange={(name) => onChange({ name })}
              placeholder="High CPU"
              maxLength={100}
              required
              autoFocus={isNew}
            />
          )}
        </FieldRow>
        <SelectField
          label="Condition"
          value={kind}
          onValueChange={(value) => onChange(changeKind(form, value as ConditionKind))}
          groups={METRIC_GROUPS.map((group) => ({
            label: group.label,
            items: group.items.map((item) => ({ value: item.kind, label: CONDITION_LABELS[item.kind] })),
          }))}
        />
        <ConditionFields form={form} onChange={onChange} />
      </div>
      {hasAggregation(kind) && (
        <SettingRow
          title="Measured As"
          description="How readings are combined before the check"
          className="py-3.5"
          control={(labelId) => (
            <PlainSelect
              labelId={labelId}
              value={measureValue}
              onValueChange={changeMeasure}
              groups={getMeasureGroups(form.windowSecs)}
            />
          )}
        />
      )}
    </FormSection>
  );
}
