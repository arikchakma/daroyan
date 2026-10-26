import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export function installTokenOptions(version: number, maxUses: number) {
  return queryOptions({
    queryKey: ['install-token', version, maxUses],
    queryFn: () => request(apiClient.POST('/v1/install-tokens', { body: { max_uses: maxUses } })),
    staleTime: Infinity,
    gcTime: Infinity,
    refetchOnWindowFocus: false,
  });
}

export function revokeInstallTokenOptions() {
  return mutationOptions({
    mutationFn: (variables: { tokenId: string }) =>
      request(
        apiClient.DELETE('/v1/install-tokens/{token_id}', {
          params: { path: { token_id: variables.tokenId } },
        })
      ),
  });
}
