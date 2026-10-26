import { queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export function overviewTrendsOptions() {
  return queryOptions({
    queryKey: ['overview', 'trends'],
    queryFn: () => request(apiClient.GET('/v1/overview/trends')),
    refetchInterval: 60_000,
  });
}
