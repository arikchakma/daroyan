import type { Schemas } from '@daroyan/client';

import { formatBytes, formatDate, formatRelative, pluralize } from '~/utils/format';

export function getRoleLabel(role: Schemas['Me']['role']) {
  if (role === 'admin') {
    return 'Admin';
  }

  return 'Member';
}

export function getArchiveLabel(archive: Schemas['Archive'] | null | undefined) {
  if (!archive) {
    return 'Off';
  }

  return `${archive.url} · Kept ${pluralize(archive.retention_days, 'day')}`;
}

export function getArchiveStatus(archive: Schemas['Archive']) {
  if (archive.last_error) {
    return `Export failed: ${archive.last_error}`;
  }
  if (!archive.exported_at) {
    return 'Not exported yet';
  }

  const size = `${formatBytes(archive.bytes)} in ${pluralize(archive.files, 'file')}`;
  const since = archive.oldest_at ? ` since ${formatDate(archive.oldest_at)}` : '';

  return `${size}${since}, exported ${formatRelative(archive.exported_at)}`;
}
