import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { FiringRow } from '~/components/alerts/firing-row';
import { getIntegrationLabel } from '~/components/alerts/integration-logo';
import { describeQuietWindows } from '~/components/alerts/quiet-hours-field';
import { MuteDialog } from '~/components/alerts/mute-dialog';
import type { MuteTarget } from '~/components/alerts/mute-dialog';
import { RuleStatusBadge } from '~/components/alerts/rule-status-badge';
import { Icon } from '~/components/icons/icon';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { DataTable } from '~/components/ui/data-table';
import { PageBar } from '~/components/ui/page-bar';
import { Menu } from '~/components/ui/menu';
import { Switch } from '~/components/ui/switch';
import { useAlertHistoryTable } from '~/hooks/use-alert-history-table';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { apiClient } from '~/lib/api-client';
import { request } from '~/lib/http';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  alertDeliveriesOptions,
  deleteAlertMuteOptions,
  deleteAlertRuleOptions,
  listAlertMutesOptions,
  listAlertsOptions,
  listIntegrationsOptions,
  testAlertRuleOptions,
  updateAlertRuleOptions,
} from '~/queries/alerts';
import { listServersOptions } from '~/queries/servers';
import { listUptimeChecksOptions } from '~/queries/uptime';
import {
  CONDITION_OPTIONS,
  describeMuteEnd,
  describeResolve,
  describeScope,
  formatForSecs,
  formatLimit,
  getAlertMute,
  getMutedService,
  getRuleState,
  INTEGRATION_KINDS,
  isCheckKind,
  isCronKind,
  isIntegrationKind,
  isWholeRuleMute,
  readSubjectName,
  readThreshold,
} from '~/utils/alerts';
import { formatClock, formatDate, formatRelative } from '~/utils/format';
import { OTLP_SERVER_ID, OTLP_SERVER_NAME } from '~/utils/logs';

const RECENT_MS = 30 * 86_400_000;

type RuleBodyProps = {
  rule: Schemas['AlertRule'];
};

export function RuleBody(props: RuleBodyProps) {
  const { rule } = props;

  const navigate = useNavigate();
  const { data: history = [] } = useQuery(listAlertsOptions('all', rule.id));
  const { data: deliveries = [] } = useQuery(alertDeliveriesOptions(rule.id));
  const { data: servers = [] } = useQuery(listServersOptions());
  const { data: checks = [] } = useQuery(listUptimeChecksOptions());
  const { data: crons = [] } = useQuery({ queryKey: ['crons'], queryFn: () => request(apiClient.GET('/v1/crons')) });
  const { data: integrations = [] } = useQuery(listIntegrationsOptions());
  const { data: allMutes = [] } = useQuery(listAlertMutesOptions());
  const isAdmin = useIsAdmin();
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const [muting, setMuting] = useState<{ target?: MuteTarget } | undefined>();

  const mutes = allMutes.filter((mute) => mute.rule_id === rule.id);
  const wholeMute = mutes.find(isWholeRuleMute);
  const firing = history.filter((alert) => alert.status === 'firing');
  const state = getRuleState(rule, firing, mutes);
  const firstMute = wholeMute ?? mutes.find((mute) => firing.some((alert) => getAlertMute([mute], alert)));
  const stateLabel =
    state === 'muted' && firstMute
      ? `Muted ${describeMuteEnd(firstMute)}`
      : state === 'limited' && rule.quiet_until
        ? `Quiet until ${formatClock(new Date(rule.quiet_until).getTime())}`
        : undefined;
  const now = Date.now();
  const recent = history.filter((alert) => now - new Date(alert.started_at).getTime() < RECENT_MS);
  const resolved = recent.filter((alert) => alert.resolved_at);
  const avgMinutes =
    resolved.length > 0
      ? resolved.reduce(
          (sum, alert) => sum + (new Date(alert.resolved_at ?? 0).getTime() - new Date(alert.started_at).getTime()),
          0
        ) /
        resolved.length /
        60_000
      : undefined;
  const isCheck = isCheckKind(rule.condition.kind);
  const isCron = isCronKind(rule.condition.kind);
  const noun = isCheck ? 'check' : isCron ? 'cron' : 'server';
  const nounTitle = isCheck ? 'Checks' : isCron ? 'Crons' : 'Servers';
  const covered: { id: string }[] = isCheck
    ? checks.filter((check) => rule.scope.kind === 'all' || (rule.scope.kind === 'checks' && rule.scope.check_ids.includes(check.id)))
    : isCron
      ? crons.filter((cron) => rule.scope.kind === 'all' || (rule.scope.kind === 'crons' && rule.scope.cron_ids.includes(cron.id)))
      : servers.filter((server) =>
        rule.scope.kind === 'all'
          ? true
          : rule.scope.kind === 'label'
            ? server.labels.includes(rule.scope.label)
            : rule.scope.kind === 'servers' && rule.scope.server_ids.includes(server.id)
      );
  const notified = integrations.filter((integration) => rule.integration_ids.includes(integration.id));
  const scopeLabel = describeScope(rule.scope, servers, checks, isCheck, crons, isCron);
  const table = useAlertHistoryTable(history, mutes);
  const unmute = useMutation({
    ...deleteAlertMuteOptions(),
    meta: { errorMessage: 'The alert could not be unmuted' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-mutes'] });
      toast.success('Unmuted');
    },
  });
  const getTargetName = (mute: Schemas['AlertMute']) => {
    if (mute.subject) {
      return mute.subject;
    }
    if (mute.server_id === OTLP_SERVER_ID) {
      return OTLP_SERVER_NAME;
    }
    if (mute.server_id) {
      return servers.find((server) => server.id === mute.server_id)?.name ?? 'A server';
    }
    if (mute.check_id) {
      return checks.find((check) => check.id === mute.check_id)?.name ?? 'A check';
    }
    if (mute.cron_id) {
      return crons.find((cron) => cron.id === mute.cron_id)?.name ?? 'A cron';
    }
    return 'Every target';
  };

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
    queryClient.invalidateQueries({ queryKey: ['alerts'] });
  };
  const update = useMutation({
    ...updateAlertRuleOptions(),
    meta: { errorMessage: 'The rule could not be updated' },
    onSuccess: (saved) => {
      refresh();
      toast.success(saved.enabled ? 'Alerts are on' : 'Alerts are off');
    },
  });
  const remove = useMutation({
    ...deleteAlertRuleOptions(),
    meta: { errorMessage: 'The rule could not be deleted' },
    onSuccess: () => {
      navigate('/alerts');
      queryClient.invalidateQueries({ queryKey: ['alert-rules', 'list'] });
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      toast.success(`Deleted ${rule.name}`);
    },
  });
  const test = useMutation({
    ...testAlertRuleOptions(),
    meta: { errorMessage: 'The test alert could not be sent' },
    onSuccess: () => toast.success('Test alert sent'),
  });

  useCommandActions(rule.name, [
    isAdmin && {
      label: 'Edit Rule',
      icon: 'pencil',
      onSelect: () => void navigate(`/alerts/${rule.id}/edit`),
    },
    isAdmin &&
      rule.integration_ids.length > 0 && {
        label: 'Send Test Alert',
        icon: 'send',
        onSelect: () =>
          test.mutate({
            name: rule.name,
            condition: rule.condition,
            for_secs: rule.for_secs,
            integration_ids: rule.integration_ids,
          }),
      },
    wholeMute
      ? { label: 'Unmute Rule', icon: 'play', onSelect: () => unmute.mutate({ muteId: wholeMute.id }) }
      : rule.enabled && { label: 'Mute Rule', icon: 'bell-off', onSelect: () => setMuting({}) },
    isAdmin && {
      label: rule.enabled ? 'Disable Rule' : 'Enable Rule',
      icon: rule.enabled ? 'bell-off' : 'bell',
      onSelect: () => update.mutate({ ruleId: rule.id, body: { enabled: !rule.enabled } }),
    },
    isAdmin && { label: 'Delete Rule', icon: 'trash', onSelect: () => setIsDeleteOpen(true) },
  ]);

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo="/alerts" icon="bell" title="Alerts">
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            More Actions
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup>
                {isAdmin && (
                  <Menu.Item
                    disabled={rule.integration_ids.length === 0}
                    onClick={() =>
                      test.mutate({
                        name: rule.name,
                        condition: rule.condition,
                        for_secs: rule.for_secs,
                        integration_ids: rule.integration_ids,
                      })
                    }
                  >
                    <Icon name="send" />
                    Send Test Alert
                  </Menu.Item>
                )}
                {wholeMute ? (
                  <Menu.Item onClick={() => unmute.mutate({ muteId: wholeMute.id })}>
                    <Icon name="play" />
                    Unmute Rule
                  </Menu.Item>
                ) : (
                  <Menu.Item disabled={!rule.enabled} onClick={() => setMuting({})}>
                    <Icon name="bell-off" />
                    Mute Rule…
                  </Menu.Item>
                )}
                {isAdmin && (
                  <>
                    <Menu.Item onClick={() => update.mutate({ ruleId: rule.id, body: { enabled: !rule.enabled } })}>
                      <Icon name={rule.enabled ? 'bell-off' : 'bell'} />
                      {rule.enabled ? 'Disable Rule' : 'Enable Rule'}
                    </Menu.Item>
                    <Menu.Separator />
                    <Menu.Item onClick={() => setIsDeleteOpen(true)} className="text-tone-critical-fg">
                      <Icon name="trash" />
                      Delete Rule
                    </Menu.Item>
                  </>
                )}
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
        {isAdmin && (
          <Button nativeButton={false} render={<Link to={`/alerts/${rule.id}/edit`} />} className="w-fit">
            Edit Rule
          </Button>
        )}
      </PageBar>

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:px-4">
        <div className="flex w-200 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2.5">
              <h1 className="truncate text-title-page font-semibold">{rule.name}</h1>
              <RuleStatusBadge state={state} label={stateLabel} />
            </div>
            <p className="text-body text-text-secondary">
              {[rule.summary, scopeLabel, `Created ${formatDate(rule.created_at)}`].join(' · ')}
            </p>
          </div>

          <StatStrip columns={4}>
            <Stat label={nounTitle} value={covered.length.toString()} detail="covered" />
            <Stat
              label="Fired"
              value={recent.length === 1 ? '1 time' : `${recent.length} times`}
              detail="30 days"
            />
            <Stat
              label="Avg. Duration"
              value={avgMinutes === undefined ? '—' : formatMinutes(avgMinutes)}
            />
            <Stat label="Last Fired" value={rule.last_fired_at ? formatRelative(rule.last_fired_at) : 'Never'} />
          </StatStrip>

          {firing.length > 0 && (
            <DetailSection
              title="Firing Now"
              summary={`${firing.length} alert${firing.length === 1 ? '' : 's'}`}
            >
              <div className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
                {firing.map((alert) => (
                  <FiringRow
                    key={alert.id}
                    alert={alert}
                    mute={getAlertMute(mutes, alert)}
                    onMute={() =>
                      setMuting({
                        target: {
                          serverId: alert.server_id,
                          checkId: alert.check_id,
                          cronId: alert.cron_id,
                          subject: getMutedService(alert),
                          name: getMutedService(alert) ?? alert.target_name,
                        },
                      })
                    }
                    onUnmute={(muteId) => unmute.mutate({ muteId })}
                    deliveries={deliveries.filter(
                      (delivery) => delivery.alert_id === alert.id && delivery.status === 'sent'
                    )}
                  />
                ))}
              </div>
            </DetailSection>
          )}

          <DetailSection
            title="History"
            summary={recent.length > 0 ? `${recent.length} ${recent.length === 1 ? 'time' : 'times'} in 30 days` : undefined}
          >
            <DetailCard>
              {history.length > 0 ? (
                <DataTable table={table} label="Alert History" variant="card" size="lg" />
              ) : (
                <DetailEmpty title="This rule has not fired yet">Each time it fires, it shows up here with how long it lasted.</DetailEmpty>
              )}
            </DetailCard>
          </DetailSection>
        </div>

        <aside aria-label="Rule" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <RailSection title="Details">
            <RailRow label="Condition">{getConditionLabel(rule.condition.kind)}</RailRow>
            <RailRow label="Critical">{describeLimit(rule.condition)}</RailRow>
            {rule.options.warning != null && (
              <RailRow label="Warning">{formatLimit(rule.condition.kind, rule.options.warning)}</RailRow>
            )}
            <RailRow label="For">{formatForSecs(rule.for_secs)}</RailRow>
            <RailRow label={nounTitle}>{scopeLabel}</RailRow>
            {rule.options.severity_label && <RailRow label="Severity">{rule.options.severity_label}</RailRow>}
            {rule.options.runbook_url && (
              <RailRow label="Runbook">
                <a href={rule.options.runbook_url} target="_blank" rel="noreferrer" className="text-text-link hover:underline">
                  Open
                </a>
              </RailRow>
            )}
          </RailSection>
          <RailSection title="Notify">
            {notified.length > 0 ? (
              notified.map((integration) => (
                <div key={integration.id} className="flex items-center gap-2 text-body">
                  <Icon name={isIntegrationKind(integration.kind) ? INTEGRATION_KINDS[integration.kind].icon : 'webhook'} />
                  <span className="min-w-0 flex-1 truncate">{integration.name}</span>
                  <span className="text-text-tertiary">{getIntegrationLabel(integration.kind)}</span>
                </div>
              ))
            ) : (
              <p className="text-body text-text-secondary">
                Nobody.{' '}
                {isAdmin && (
                  <Link to={`/alerts/${rule.id}/edit`} className="text-text-link hover:underline">
                    Pick an integration
                  </Link>
                )}
              </p>
            )}
          </RailSection>
          <RailSection title="Status" isLast={!isAdmin}>
            <div className="flex items-center gap-3 text-body">
              <span id="send-alerts" className="flex-1 text-text-secondary">
                Send alerts
              </span>
              <Switch.Root
                aria-labelledby="send-alerts"
                checked={rule.enabled}
                disabled={!isAdmin || update.isPending}
                onCheckedChange={(enabled) => update.mutate({ ruleId: rule.id, body: { enabled } })}
              >
                <Switch.Thumb />
              </Switch.Root>
            </div>
            {mutes.map((mute) => (
              <p key={mute.id} className="text-body text-text-secondary">
                {getTargetName(mute)} muted {describeMuteEnd(mute)}
              </p>
            ))}
            {rule.options.message_limit != null && (
              <p className="text-body text-text-secondary">At most {rule.options.message_limit} messages an hour</p>
            )}
            {rule.options.quiet_hours.length > 0 && (
              <p className="text-body text-text-secondary">
                Quiet {describeQuietWindows(rule.options.quiet_hours, rule.options.time_zone ?? 'UTC')}
              </p>
            )}
          </RailSection>
          {isAdmin && (
            <RailSection title="Delete Rule" isLast>
              <Button
                variant="secondary"
                className="w-fit text-tone-critical-fg"
                onClick={() => setIsDeleteOpen(true)}
              >
                Delete Rule
              </Button>
            </RailSection>
          )}
        </aside>
      </div>

      {muting && (
        <MuteDialog
          open
          onOpenChange={(open) => !open && setMuting(undefined)}
          ruleId={rule.id}
          ruleName={rule.name}
          ruleNoun={noun}
          resolveHint={describeResolve(rule.condition)}
          target={muting.target}
          isFiring={firing.length > 0}
        />
      )}

      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${rule.name}?`}
        description="Its history goes too. A firing alert stops without a resolve message."
        confirmLabel="Delete Rule"
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ ruleId: rule.id })}
      />
    </div>
  );
}

function formatMinutes(minutes: number) {
  if (minutes < 1) {
    return '<1m';
  }
  if (minutes < 60) {
    return `${Math.round(minutes)}m`;
  }
  return `${(minutes / 60).toFixed(1)}h`;
}

function getConditionLabel(kind: Schemas['Condition']['kind']) {
  const label = CONDITION_OPTIONS.find((option) => option.kind === kind)?.label ?? kind;
  return label.replace(' a limit', '');
}

function describeLimit(condition: Schemas['Condition']) {
  const limit = readThreshold(condition);
  if (limit !== undefined) {
    return formatLimit(condition.kind, limit);
  }
  if (condition.kind === 'offline') {
    return 'No report';
  }
  if (condition.kind === 'uptime_down') {
    return 'Check down';
  }
  return readSubjectName(condition) ?? `Any ${condition.kind === 'process_stopped' ? 'process' : 'port'}`;
}
