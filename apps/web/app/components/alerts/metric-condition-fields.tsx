import { useQuery } from '@tanstack/react-query';

import { FieldGrid } from '~/components/alerts/field-grid';
import { ForField } from '~/components/alerts/for-field';
import { SelectField } from '~/components/alerts/select-field';
import { UnitField } from '~/components/alerts/unit-field';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { metricQueries } from '~/queries/metrics';
import { TRACE_WINDOW_OPTIONS } from '~/utils/alerts';
import { getAggregations, getMetricUnitWord } from '~/utils/metrics';
import { getMetricLimitHint, isLimitValid, isOptionalNumber } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

const DIRECTION_OPTIONS = [
  { value: 'above', label: 'Above' },
  { value: 'below', label: 'Below' },
];

type MetricConditionFieldsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
};

export function MetricConditionFields(props: MetricConditionFieldsProps) {
  const { form, onChange } = props;

  const { data: metrics = [] } = useQuery(metricQueries.list());
  const { data: attributes = [] } = useQuery({
    ...metricQueries.attributes(form.metricName),
    enabled: !!form.metricName,
  });

  const metric = metrics.find((item) => item.name === form.metricName);
  const aggregations = metric ? getAggregations(metric) : [];
  const unit = metric ? getMetricUnitWord(metric.unit) : '';
  const isLimitWrong = form.critical !== '' && !isLimitValid(form);
  const groupItems = [
    { value: 'none', label: 'Nothing' },
    { value: 'service', label: 'Each service' },
    { value: 'server', label: 'Each server' },
    ...attributes.map((attribute) => ({ value: attribute.key, label: `Each ${attribute.key}` })),
  ];
  const metricItems = [
    ...(form.metricName && !metric ? [{ value: form.metricName, label: form.metricName }] : []),
    ...metrics.map((item) => ({ value: item.name, label: item.name })),
  ];

  return (
    <>
      <FieldGrid columns={2}>
        <SelectField
          label="Metric"
          value={form.metricName || null}
          onValueChange={(value) =>
            onChange({ metricName: value ?? '', metricAggregation: '', metricFilter: '', metricGroupBy: '' })
          }
          items={metricItems}
          placeholder={metrics.length === 0 ? 'No metrics yet' : 'Pick a metric'}
        />
        <SelectField
          label="Read As"
          value={form.metricAggregation || aggregations[0]?.value || null}
          onValueChange={(value) =>
            onChange({ metricAggregation: aggregations.find((item) => item.value === value)?.value ?? '' })
          }
          items={aggregations}
        />
      </FieldGrid>
      <FieldGrid columns={2}>
        <SelectField
          label="Service"
          value={form.metricService || 'any'}
          onValueChange={(value) => onChange({ metricService: !value || value === 'any' ? '' : value })}
          items={[
            { value: 'any', label: 'Any service' },
            ...(metric?.services ?? []).map((service) => ({ value: service, label: service })),
          ]}
        />
        <SelectField
          label="Group By"
          value={form.metricGroupBy || 'none'}
          onValueChange={(value) => onChange({ metricGroupBy: !value || value === 'none' ? '' : value })}
          items={groupItems}
        />
      </FieldGrid>
      <FieldRow label="Filter" hint="Optional. Attribute filters separated by commas, like queue=email or status!=200.">
        {({ inputId, hintId }) => (
          <Input
            id={inputId}
            aria-describedby={hintId}
            value={form.metricFilter}
            onValueChange={(metricFilter) => onChange({ metricFilter })}
            placeholder="Every series"
            maxLength={200}
          />
        )}
      </FieldRow>
      <FieldGrid columns={form.isWarningOpen ? 3 : 2}>
        <SelectField
          label="Fires When"
          value={form.metricBelow ? 'below' : 'above'}
          onValueChange={(value) => onChange({ metricBelow: value === 'below' })}
          items={DIRECTION_OPTIONS}
        />
        <UnitField
          label={form.isWarningOpen ? 'Critical Limit' : 'Limit'}
          unit={unit}
          value={form.critical}
          onValueChange={(critical) => onChange({ critical })}
          isInvalid={isLimitWrong}
        />
        {form.isWarningOpen && (
          <UnitField
            label="Warning Limit"
            unit={unit}
            value={form.warning}
            onValueChange={(warning) => onChange({ warning })}
            placeholder="Off"
            isInvalid={!isOptionalNumber(form.warning)}
          />
        )}
      </FieldGrid>
      <FieldGrid columns={2} hint={metric && isLimitValid(form) ? getMetricLimitHint(form, metric) : undefined}>
        <SelectField
          label="Within"
          value={form.metricWithin.toString()}
          onValueChange={(value) => onChange({ metricWithin: Number(value) })}
          items={TRACE_WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
        />
        <ForField kind={form.kind} value={form.forSecs} onValueChange={(forSecs) => onChange({ forSecs })} />
      </FieldGrid>
    </>
  );
}
