import type { Schemas } from '@daroyan/client';

import { MetricSideCard } from '~/components/metrics/metric-side-card';
import { MetricSideRow } from '~/components/metrics/metric-side-row';
import { readMetricCondition } from '~/utils/alerts';
import { getMetricRuleText } from '~/utils/metrics';

const VISIBLE_RULES = 4;

type MetricRulesCardProps = {
  rules: Schemas['AlertRule'][];
  firingRuleIds: string[];
};

export function MetricRulesCard(props: MetricRulesCardProps) {
  const { rules, firingRuleIds } = props;

  const sorted = [...rules].sort(
    (a, b) => Number(firingRuleIds.includes(b.id)) - Number(firingRuleIds.includes(a.id))
  );

  return (
    <MetricSideCard
      title="Alert Rules"
      linkLabel="View Alerts"
      linkTo="/alerts"
      empty={rules.length === 0 ? 'No rules watch this metric yet.' : undefined}
    >
      {sorted.slice(0, VISIBLE_RULES).map((rule) => {
        const condition = readMetricCondition(rule.condition);
        return (
          <MetricSideRow
            key={rule.id}
            to={`/alerts/${rule.id}`}
            label={rule.name}
            detail={condition ? getMetricRuleText(condition, rule.for_secs) : ''}
            isCritical={firingRuleIds.includes(rule.id)}
          />
        );
      })}
    </MetricSideCard>
  );
}
