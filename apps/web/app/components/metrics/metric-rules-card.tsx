import type { Schemas } from '@daroyan/client';

import { MetricSideCard } from '~/components/metrics/metric-side-card';
import { MetricSideRow } from '~/components/metrics/metric-side-row';
import { getMetricRuleText } from '~/utils/alerts';
import type { MetricSummary } from '~/utils/metrics';

const VISIBLE_RULES = 4;

type MetricRulesCardProps = {
  metric: MetricSummary;
  rules: Schemas['AlertRule'][];
  firingRuleIds: string[];
};

export function MetricRulesCard(props: MetricRulesCardProps) {
  const { metric, rules, firingRuleIds } = props;

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
      {sorted.slice(0, VISIBLE_RULES).map((rule) => (
        <MetricSideRow
          key={rule.id}
          to={`/alerts/${rule.id}`}
          label={rule.name}
          detail={getMetricRuleText(rule, metric)}
          isFiring={firingRuleIds.includes(rule.id)}
        />
      ))}
    </MetricSideCard>
  );
}
