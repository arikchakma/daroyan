import type { Schemas } from '@daroyan/client';
import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { ApiError, request } from '~/lib/http';

const REFRESH_MS = 10_000;
const COMMAND_POLL_MS = 1_000;
const COMMAND_POLL_LIMIT = 30;

export type MetricRange = '1h' | '6h' | '24h' | '7d' | '30d';
export type CommandAction = Schemas['ActionName'];

export function listServersOptions() {
  return queryOptions({
    queryKey: ['servers'],
    queryFn: () => request(apiClient.GET('/v1/servers')),
    refetchInterval: REFRESH_MS,
  });
}

export function serverOptions(serverId: string) {
  return queryOptions({
    queryKey: ['servers', serverId],
    queryFn: () =>
      request(
        apiClient.GET('/v1/servers/{server_id}', {
          params: { path: { server_id: serverId } },
        })
      ),
    refetchInterval: REFRESH_MS,
  });
}

export function serverMetricsOptions(serverId: string, range: MetricRange) {
  return queryOptions({
    queryKey: ['servers', serverId, 'metrics', range],
    queryFn: () =>
      request(
        apiClient.GET('/v1/servers/{server_id}/metrics', {
          params: { path: { server_id: serverId }, query: { range } },
        })
      ),
    refetchInterval: REFRESH_MS,
  });
}

export function serverEventsOptions(serverId: string) {
  return queryOptions({
    queryKey: ['servers', serverId, 'events'],
    queryFn: () =>
      request(
        apiClient.GET('/v1/servers/{server_id}/events', {
          params: { path: { server_id: serverId } },
        })
      ),
    refetchInterval: REFRESH_MS,
  });
}

export function renameServerOptions() {
  return mutationOptions({
    mutationFn: (variables: { serverId: string; name: string }) =>
      request(
        apiClient.PATCH('/v1/servers/{server_id}', {
          params: { path: { server_id: variables.serverId } },
          body: { name: variables.name },
        })
      ),
  });
}

export function deleteServerOptions() {
  return mutationOptions({
    mutationFn: (variables: { serverId: string }) =>
      request(
        apiClient.DELETE('/v1/servers/{server_id}', {
          params: { path: { server_id: variables.serverId } },
        })
      ),
  });
}

type RunCommandVariables = {
  serverId: string;
  action: CommandAction;
  name: string;
};

async function runCommand(variables: RunCommandVariables) {
  const { serverId, action, name } = variables;
  const command = await request(
    apiClient.POST('/v1/servers/{server_id}/commands', {
      params: { path: { server_id: serverId } },
      body: { action, name },
    })
  );

  for (let attempt = 0; attempt < COMMAND_POLL_LIMIT; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, COMMAND_POLL_MS));
    const current = await request(
      apiClient.GET('/v1/servers/{server_id}/commands/{command_id}', {
        params: { path: { server_id: serverId, command_id: command.id } },
      })
    );
    if (current.status === 'succeeded') {
      return current;
    }
    if (current.status === 'failed') {
      throw new ApiError('command_failed', current.error ?? 'The command failed', 500);
    }
  }
  throw new ApiError('command_timeout', 'The agent did not answer in time', 504);
}

export function runCommandOptions() {
  return mutationOptions({
    mutationFn: runCommand,
  });
}
