import { queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export const instanceQueries = {
  all: () => ['instance'] as const,
  detail: () =>
    queryOptions({
      queryKey: [...instanceQueries.all(), 'detail'],
      queryFn: () => request(apiClient.GET('/v1/instance')),
    }),
};
