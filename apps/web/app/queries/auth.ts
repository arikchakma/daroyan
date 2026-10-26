import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export function meOptions() {
  return queryOptions({
    queryKey: ['me'],
    queryFn: () => request(apiClient.GET('/v1/auth/me')),
    staleTime: Infinity,
  });
}

export function googleLoginOptions() {
  return mutationOptions({
    mutationFn: (next?: string) =>
      request(apiClient.POST('/v1/auth/google', { params: { query: { next } } })),
  });
}
