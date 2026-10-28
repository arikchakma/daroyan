import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export type InstallTokenLimits = {
  expires_in_hours: number | null;
  max_uses: number | null;
};

export const ONE_OFF_LIMITS: InstallTokenLimits = { expires_in_hours: 24, max_uses: 1 };

export function installTokenOptions(version: number, limits: InstallTokenLimits) {
  return queryOptions({
    queryKey: ['install-token', version, limits],
    queryFn: () => request(apiClient.POST('/v1/install-tokens', { body: { name: 'Add Server', ...limits } })),
    staleTime: Infinity,
    gcTime: Infinity,
    refetchOnWindowFocus: false,
  });
}

export function listInstallTokensOptions() {
  return queryOptions({
    queryKey: ['install-tokens'],
    queryFn: () => request(apiClient.GET('/v1/install-tokens')),
  });
}

export function createInstallTokenOptions(limits: InstallTokenLimits) {
  return mutationOptions({
    mutationFn: (body: { name: string }) =>
      request(apiClient.POST('/v1/install-tokens', { body: { ...body, ...limits } })),
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
