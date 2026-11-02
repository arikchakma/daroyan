import { queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export const overviewQueries = {
  all: () => ['overview'] as const,
  trends: () =>
    queryOptions({
      queryKey: [...overviewQueries.all(), 'trends'],
      queryFn: () => request(apiClient.GET('/v1/overview/trends')),
      refetchInterval: 60_000,
    }),
  events: () =>
    queryOptions({
      queryKey: [...overviewQueries.all(), 'events'],
      queryFn: () => request(apiClient.GET('/v1/overview/events')),
      refetchInterval: 60_000,
    }),
};
