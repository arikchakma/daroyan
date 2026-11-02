import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export const backupQueries = {
  all: () => ['backups'] as const,
  status: () =>
    queryOptions({
      queryKey: [...backupQueries.all(), 'status'],
      queryFn: () => request(apiClient.GET('/v1/backups')),
    }),
};

export function createBackupOptions() {
  return mutationOptions({
    mutationFn: () => request(apiClient.POST('/v1/backups')),
  });
}
