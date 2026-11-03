import { queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export const overviewQueries = {
  all: () => ['overview'] as const,
  events: () =>
    queryOptions({
      queryKey: [...overviewQueries.all(), 'events'],
      queryFn: () => request(apiClient.GET('/v1/overview/events')),
      refetchInterval: 60_000,
    }),
  upcoming: () =>
    queryOptions({
      queryKey: [...overviewQueries.all(), 'upcoming'],
      queryFn: () => request(apiClient.GET('/v1/overview/upcoming')),
      refetchInterval: 5 * 60_000,
    }),
};
