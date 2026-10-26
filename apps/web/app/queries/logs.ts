import { keepPreviousData, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export type LogsParams = {
  server_id?: string;
  server_op?: 'not';
  service?: string;
  service_op?: 'not';
  method?: string;
  method_op?: 'not';
  status?: string;
  status_op?: 'not';
  level?: LogLevel;
  q?: string;
  since?: number;
  until?: number;
  offset: number;
  limit: number;
};

export function logsOptions(params: LogsParams, isLive: boolean) {
  return queryOptions({
    queryKey: ['logs', params, isLive],
    queryFn: () => request(apiClient.GET('/v1/logs', { params: { query: params } })),
    placeholderData: keepPreviousData,
    refetchInterval: isLive ? 2_000 : false,
  });
}

export function logSourcesOptions() {
  return queryOptions({
    queryKey: ['log-sources'],
    queryFn: () => request(apiClient.GET('/v1/logs/sources')),
  });
}

export function logContextOptions(line: { server_id: string; service: string; at: number }) {
  return queryOptions({
    queryKey: ['log-context', line],
    queryFn: () => request(apiClient.GET('/v1/logs/context', { params: { query: line } })),
  });
}
