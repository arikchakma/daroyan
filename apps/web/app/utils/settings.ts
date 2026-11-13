import type { Schemas } from '@daroyan/client';

export function getRoleLabel(role: Schemas['Me']['role']) {
  if (role === 'admin') {
    return 'Admin';
  }

  return 'Member';
}

export function getIngestTokenDetail(token: Schemas['IngestToken']) {
  const parts = [
    token.throttled_log_records > 0 &&
      `${token.throttled_log_records.toLocaleString()} log records throttled over the budget`,
    token.rejected_metric_points > 0 &&
      `${token.rejected_metric_points.toLocaleString()} metric points dropped over the series limit`,
  ].filter(Boolean);

  return parts.length > 0 ? parts.join(' · ') : undefined;
}
