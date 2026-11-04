import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';

import { alertMuteQueries, alertQueries, alertRuleQueries } from '~/queries/alerts';
import { getAlertMute, getAlertsForTarget } from '~/utils/alerts';
import type { AlertTarget } from '~/utils/alerts';

export function useFiringAlerts(target?: AlertTarget) {
  const { data: firing = [], isSuccess: isLoaded } = useQuery(alertQueries.list('firing'));
  const { data: mutes = [] } = useQuery(alertMuteQueries.list());
  const { data: rules = [] } = useQuery(alertRuleQueries.list());

  const alerts = target ? getAlertsForTarget(firing, target) : firing;
  const getMute = (alert: Schemas['Alert']) => getAlertMute(mutes, alert);
  const getRule = (alert: Schemas['Alert']) => rules.find((rule) => rule.id === alert.rule_id);
  const active = alerts.filter((alert) => !getMute(alert));

  return { alerts, active, mutes, isLoaded, getMute, getRule };
}
