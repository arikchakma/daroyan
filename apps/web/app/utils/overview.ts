import type { Schemas } from '@daroyan/client';

import type { IconName } from '~/components/icons/icon-paths';
import { getAlertTargetPath } from '~/utils/alerts';
import { formatDate } from '~/utils/format';
import { OTLP_SERVER_ID } from '~/utils/logs';
import { METRICS_TARGET_ID } from '~/utils/metrics';
import { TRACES_TARGET_ID } from '~/utils/traces';

export function getAlertHref(alert: Schemas['Alert']) {
  if (alert.server_id === OTLP_SERVER_ID || alert.server_id === TRACES_TARGET_ID || alert.server_id === METRICS_TARGET_ID) {
    return getAlertTargetPath(alert);
  }
  if (alert.server_id) {
    return `/servers/${alert.server_id}`;
  }
  if (alert.check_id) {
    return `/uptime/${alert.check_id}`;
  }
  if (alert.cron_id) {
    return `/crons/${alert.cron_id}`;
  }
  return `/alerts/${alert.rule_id}`;
}

export function getUpcomingHref(item: Schemas['UpcomingItem']) {
  if (item.kind === 'disk_full') {
    return `/servers/${item.target_id}`;
  }
  if (item.kind === 'cert_expiry') {
    return `/uptime/${item.target_id}`;
  }
  if (item.kind === 'maintenance') {
    return `/incidents/${item.target_id}`;
  }
  return '/settings/agents';
}

export function getUpcomingIcon(kind: Schemas['UpcomingKind']): IconName {
  if (kind === 'disk_full') {
    return 'database';
  }
  if (kind === 'cert_expiry') {
    return 'lock';
  }
  if (kind === 'maintenance') {
    return 'calendar';
  }
  return 'download';
}

export function getUpcomingDate(item: Schemas['UpcomingItem']) {
  return typeof item.at === 'number' ? formatDate(item.at) : undefined;
}

