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

export type LogVolumeParams = Omit<LogsParams, 'offset' | 'limit'>;

export const logQueries = {
  all: () => ['logs'] as const,
  list: (params: LogsParams, isLive: boolean) =>
    queryOptions({
      queryKey: [...logQueries.all(), 'list', params, isLive],
      queryFn: () => request(apiClient.GET('/v1/logs', { params: { query: params } })),
      placeholderData: keepPreviousData,
      refetchInterval: isLive ? 2_000 : false,
    }),
  volume: (params: LogVolumeParams, isLive: boolean) =>
    queryOptions({
      queryKey: [...logQueries.all(), 'volume', params, isLive],
      queryFn: () => request(apiClient.GET('/v1/logs/volume', { params: { query: params } })),
      placeholderData: keepPreviousData,
      refetchInterval: isLive ? 10_000 : false,
    }),
  sources: () =>
    queryOptions({
      queryKey: [...logQueries.all(), 'sources'],
      queryFn: () => request(apiClient.GET('/v1/logs/sources')),
    }),
  context: (line: { server_id: string; service: string; at: number }) =>
    queryOptions({
      queryKey: [...logQueries.all(), 'context', line],
      queryFn: () => request(apiClient.GET('/v1/logs/context', { params: { query: line } })),
    }),
};
