import type { Schemas } from '@daroyan/client';
import { mutationOptions, queryOptions } from '@tanstack/react-query';

import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';

const REFRESH_MS = 15_000;

export type AlertStatus = 'firing' | 'resolved' | 'all';
export type AlertRuleInput = Schemas['CreateAlertRuleBody'];
export type IntegrationConfig = Schemas['IntegrationConfig'];

export function listAlertsOptions(status: AlertStatus, ruleId?: string) {
  return queryOptions({
    queryKey: ['alerts', status, ruleId ?? null],
    queryFn: () => request(apiClient.GET('/v1/alerts', { params: { query: { status, rule_id: ruleId } } })),
    refetchInterval: REFRESH_MS,
  });
}

export function alertRuleOptions(ruleId: string) {
  return queryOptions({
    queryKey: ['alert-rules', ruleId],
    queryFn: () =>
      request(apiClient.GET('/v1/alert-rules/{rule_id}', { params: { path: { rule_id: ruleId } } })),
  });
}

export function alertDeliveriesOptions(ruleId: string) {
  return queryOptions({
    queryKey: ['alert-rules', ruleId, 'deliveries'],
    queryFn: () =>
      request(
        apiClient.GET('/v1/alert-rules/{rule_id}/deliveries', { params: { path: { rule_id: ruleId } } })
      ),
    refetchInterval: REFRESH_MS,
  });
}

export function backtestAlertRuleOptions(body: Schemas['BacktestAlertRuleBody']) {
  return queryOptions({
    queryKey: ['alert-rules', 'backtest', body],
    queryFn: () => request(apiClient.POST('/v1/alert-rules/backtest', { body })),
    staleTime: 60_000,
  });
}

export function testAlertRuleOptions() {
  return mutationOptions({
    mutationFn: (body: Schemas['TestAlertRuleBody']) => request(apiClient.POST('/v1/alert-rules/test', { body })),
  });
}

export function listAlertRulesOptions() {
  return queryOptions({
    queryKey: ['alert-rules', 'list'],
    queryFn: () => request(apiClient.GET('/v1/alert-rules')),
  });
}

export function createAlertRuleOptions() {
  return mutationOptions({
    mutationFn: (body: AlertRuleInput) => request(apiClient.POST('/v1/alert-rules', { body })),
  });
}

export function updateAlertRuleOptions() {
  return mutationOptions({
    mutationFn: (variables: { ruleId: string; body: Schemas['UpdateAlertRuleBody'] }) =>
      request(
        apiClient.PATCH('/v1/alert-rules/{rule_id}', {
          params: { path: { rule_id: variables.ruleId } },
          body: variables.body,
        })
      ),
  });
}

export function deleteAlertRuleOptions() {
  return mutationOptions({
    mutationFn: (variables: { ruleId: string }) =>
      request(
        apiClient.DELETE('/v1/alert-rules/{rule_id}', {
          params: { path: { rule_id: variables.ruleId } },
        })
      ),
  });
}

export function listIntegrationsOptions() {
  return queryOptions({
    queryKey: ['integrations'],
    queryFn: () => request(apiClient.GET('/v1/integrations')),
  });
}

export function createIntegrationOptions() {
  return mutationOptions({
    mutationFn: (body: Schemas['CreateIntegrationBody']) =>
      request(apiClient.POST('/v1/integrations', { body })),
  });
}

export function updateIntegrationOptions() {
  return mutationOptions({
    mutationFn: (variables: { integrationId: string; body: Schemas['UpdateIntegrationBody'] }) =>
      request(
        apiClient.PATCH('/v1/integrations/{integration_id}', {
          params: { path: { integration_id: variables.integrationId } },
          body: variables.body,
        })
      ),
  });
}

export function deleteIntegrationOptions() {
  return mutationOptions({
    mutationFn: (variables: { integrationId: string }) =>
      request(
        apiClient.DELETE('/v1/integrations/{integration_id}', {
          params: { path: { integration_id: variables.integrationId } },
        })
      ),
  });
}

export function testIntegrationOptions() {
  return mutationOptions({
    mutationFn: (variables: { integrationId: string }) =>
      request(
        apiClient.POST('/v1/integrations/{integration_id}/test', {
          params: { path: { integration_id: variables.integrationId } },
        })
      ),
  });
}

export function testIntegrationConfigOptions() {
  return mutationOptions({
    mutationFn: (config: IntegrationConfig) =>
      request(apiClient.POST('/v1/integrations/test', { body: { config } })),
  });
}

export type ConnectProvider = 'slack' | 'discord' | 'telegram';

export function integrationProvidersOptions() {
  return queryOptions({
    queryKey: ['integrations', 'providers'],
    queryFn: () => request(apiClient.GET('/v1/integrations/providers')),
    staleTime: 60_000,
  });
}

export function connectProviderOptions() {
  return mutationOptions({
    mutationFn: (provider: ConnectProvider) => {
      if (provider === 'slack') {
        return request(apiClient.POST('/v1/integrations/slack/connect'));
      }
      if (provider === 'discord') {
        return request(apiClient.POST('/v1/integrations/discord/connect'));
      }
      return request(apiClient.POST('/v1/integrations/telegram/connect'));
    },
  });
}

export function telegramConnectOptions(state: string) {
  return queryOptions({
    queryKey: ['integrations', 'telegram', state],
    queryFn: () =>
      request(apiClient.GET('/v1/integrations/telegram/connect/{state}', { params: { path: { state } } })),
    refetchInterval: (query) => (query.state.data?.is_connected ? false : 2_000),
  });
}

export function slackInstallOptions(installId: string) {
  return queryOptions({
    queryKey: ['integrations', 'slack-install', installId],
    queryFn: () =>
      request(
        apiClient.GET('/v1/integrations/slack/installs/{install_id}', {
          params: { path: { install_id: installId } },
        })
      ),
    staleTime: Infinity,
  });
}

export function finishSlackInstallOptions() {
  return mutationOptions({
    mutationFn: (variables: { installId: string; channelId: string }) =>
      request(
        apiClient.POST('/v1/integrations/slack/installs/{install_id}', {
          params: { path: { install_id: variables.installId } },
          body: { channel_id: variables.channelId },
        })
      ),
  });
}

export function listAlertMutesOptions() {
  return queryOptions({
    queryKey: ['alert-mutes'],
    queryFn: () => request(apiClient.GET('/v1/alert-mutes')),
    refetchInterval: REFRESH_MS,
  });
}

export function createAlertMuteOptions() {
  return mutationOptions({
    mutationFn: (body: Schemas['CreateAlertMuteBody']) => request(apiClient.POST('/v1/alert-mutes', { body })),
  });
}

export function deleteAlertMuteOptions() {
  return mutationOptions({
    mutationFn: (variables: { muteId: string }) =>
      request(apiClient.DELETE('/v1/alert-mutes/{mute_id}', { params: { path: { mute_id: variables.muteId } } })),
  });
}
