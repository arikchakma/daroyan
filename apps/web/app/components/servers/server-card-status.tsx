import type { Schemas } from '@daroyan/client';

import { useFiringAlerts } from '~/hooks/use-firing-alerts';
import { describeAlertNow } from '~/utils/alerts';
import { formatRelative } from '~/utils/format';

type ServerCardStatusProps = {
  server: Schemas['ServerItem'];
};

export function ServerCardStatus(props: ServerCardStatusProps) {
  const { server } = props;

  const { active, getRule } = useFiringAlerts({ serverId: server.id });

  if (server.status === 'pending' || !server.last_seen_at) {
    return <p className="text-body text-text-tertiary">Waiting for the first report</p>;
  }

  const lastReport = `Last report ${formatRelative(server.last_seen_at)}`;
  if (server.status === 'down') {
    return <p className="text-body text-tone-critical-fg">{lastReport}</p>;
  }

  const alert = active[0];
  if (alert) {
    return <p className="truncate text-body text-tone-warning-fg">{describeAlertNow(alert, getRule(alert)?.summary)}</p>;
  }

  return <p className="text-body text-text-tertiary">{lastReport}</p>;
}
