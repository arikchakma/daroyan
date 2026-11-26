import { useQuery } from '@tanstack/react-query';

import { FieldGrid } from '~/components/alerts/field-grid';
import { ForField } from '~/components/alerts/for-field';
import { SelectField } from '~/components/alerts/select-field';
import { UnitField } from '~/components/alerts/unit-field';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { metricQueries } from '~/queries/metrics';
import { TRACE_WINDOW_OPTIONS } from '~/utils/alerts';
import { getAggregations, getDefaultAggregation, getMetricLimitUnit } from '~/utils/metrics';
import { getMetricLimitHint, isLimitValid, isOptionalNumber } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

const MAX_FILTERS = 10;

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
  const aggregation = form.metricAggregation || (metric ? getDefaultAggregation(metric) : null);
  const unit = metric && aggregation ? getMetricLimitUnit(metric.unit, aggregation) : '';
  const isLimitWrong = form.critical !== '' && !isLimitValid(form);
  const groupItems = [
    { value: 'none', label: 'Nothing' },
    { value: 'service', label: 'Each service' },
    { value: 'server', label: 'Each server' },
    ...attributes.map((attribute) => ({ value: attribute.key, label: `Each ${attribute.key}` })),
  ];
  const services = metric?.services ?? [];
  const serviceItems = [
    { value: 'any', label: 'Any service' },
    ...(form.metricService && !services.includes(form.metricService)
      ? [{ value: form.metricService, label: form.metricService }]
      : []),
    ...services.map((service) => ({ value: service, label: service })),
  ];
  const filterRows = form.metricFilters.length > 0 ? form.metricFilters : [''];
  const metricItems = [
    ...(form.metricName && !metric ? [{ value: form.metricName, label: form.metricName }] : []),
    ...metrics.map((item) => ({ value: item.name, label: item.name })),
  ];

  const setFilter = (index: number, value: string) =>
    onChange({ metricFilters: filterRows.map((filter, other) => (other === index ? value : filter)) });

  return (
    <>
      <FieldGrid columns={2}>
        <SelectField
          label="Metric"
          value={form.metricName || null}
          onValueChange={(value) =>
            onChange({
              metricName: value ?? '',
              metricAggregation: '',
              metricService: '',
              metricFilters: [],
              metricGroupBy: '',
            })
          }
          items={metricItems}
          placeholder={metrics.length === 0 ? 'No metrics yet' : 'Pick a metric'}
        />
        <SelectField
          label="Read As"
          value={aggregation}
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
          items={serviceItems}
        />
        <SelectField
          label="Group By"
          value={form.metricGroupBy || 'none'}
          onValueChange={(value) => onChange({ metricGroupBy: !value || value === 'none' ? '' : value })}
          items={groupItems}
        />
      </FieldGrid>
      <FieldRow label="Filters" hint="Optional. One filter per row, like queue=email or status!=200.">
        {({ inputId, hintId }) => (
          <div className="flex flex-col gap-2">
            {filterRows.map((filter, index) => (
              <div key={index} className="flex items-center gap-2">
                <Input
                  id={index === 0 ? inputId : undefined}
                  aria-label={index === 0 ? undefined : `Filter ${index + 1}`}
                  aria-describedby={hintId}
                  value={filter}
                  onValueChange={(value) => setFilter(index, value)}
                  placeholder={index === 0 ? 'Every series' : undefined}
                  maxLength={200}
                />
                {filterRows.length > 1 && (
                  <Button
                    type="button"
                    variant="plain"
                    size="icon"
                    aria-label={`Remove filter ${index + 1}`}
                    onClick={() => onChange({ metricFilters: filterRows.filter((_, other) => other !== index) })}
                  >
                    <Icon name="x" />
                  </Button>
                )}
              </div>
            ))}
            {filterRows.length < MAX_FILTERS && (
              <Button
                type="button"
                variant="tertiary"
                className="w-fit"
                onClick={() => onChange({ metricFilters: [...filterRows, ''] })}
              >
                Add Filter
              </Button>
            )}
          </div>
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
          isSigned
        />
        {form.isWarningOpen && (
          <UnitField
            label="Warning Limit"
            unit={unit}
            value={form.warning}
            onValueChange={(warning) => onChange({ warning })}
            placeholder="Off"
            isInvalid={!isOptionalNumber(form.warning)}
            isSigned
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
