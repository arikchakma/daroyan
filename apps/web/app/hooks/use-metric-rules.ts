import { useQuery } from '@tanstack/react-query';

import { alertRuleQueries } from '~/queries/alerts';
import { readMetricCondition } from '~/utils/alerts';

export function useMetricRules(name: string) {
  const { data: rules = [], isSuccess } = useQuery(alertRuleQueries.list());
  const own = rules.filter((rule) => readMetricCondition(rule.condition)?.name === name);

  return { rules: own, ruleIds: own.map((rule) => rule.id), isLoaded: isSuccess };
}
