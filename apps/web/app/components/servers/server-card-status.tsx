import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';

import { alertMuteQueries, alertQueries, alertRuleQueries } from '~/queries/alerts';
import { describeAlertNow, getAlertMute } from '~/utils/alerts';
import { formatRelative } from '~/utils/format';

type ServerCardStatusProps = {
  server: Schemas['ServerItem'];
};

export function ServerCardStatus(props: ServerCardStatusProps) {
  const { server } = props;

  const { data: firing = [] } = useQuery(alertQueries.list('firing'));
  const { data: mutes = [] } = useQuery(alertMuteQueries.list());
  const { data: rules = [] } = useQuery(alertRuleQueries.list());

  if (server.status === 'pending' || !server.last_seen_at) {
    return <p className="text-body text-text-tertiary">Waiting for the first report</p>;
  }

  const lastReport = `Last report ${formatRelative(server.last_seen_at)}`;
  if (server.status === 'down') {
    return <p className="text-body text-tone-critical-fg">{lastReport}</p>;
  }

  const alert = firing.find((item) => item.server_id === server.id && !getAlertMute(mutes, item));
  if (alert) {
    const summary = rules.find((rule) => rule.id === alert.rule_id)?.summary;
    return <p className="truncate text-body text-tone-warning-fg">{describeAlertNow(alert, summary)}</p>;
  }

  return <p className="text-body text-text-tertiary">{lastReport}</p>;
}
