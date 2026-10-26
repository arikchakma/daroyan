import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { getBrowserTimeZone } from '~/components/alerts/quiet-hours-field';
import { getMuteEnd, MuteDialog, useCreateMute } from '~/components/alerts/mute-dialog';
import type { MuteLength, MuteTarget } from '~/components/alerts/mute-dialog';
import { Icon } from '~/components/icons/icon';
import { DeclareIncidentSheet } from '~/components/incidents/declare-incident-sheet';
import type { DeclareDefaults } from '~/components/incidents/declare-incident-sheet';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { deleteAlertMuteOptions, listAlertMutesOptions, listAlertRulesOptions, listAlertsOptions } from '~/queries/alerts';
import {
  describeAlertNow,
  describeMuteEnd,
  describeResolve,
  findMute,
  formatMinutesLong,
  isWholeRuleMute,
} from '~/utils/alerts';
import { formatClock } from '~/utils/format';

const QUICK_LENGTHS: { value: Exclude<MuteLength, 'custom'>; label: string }[] = [
  { value: '1h', label: 'For 1 hour' },
  { value: '4h', label: 'For 4 hours' },
  { value: 'tomorrow', label: 'Until tomorrow, 09:00' },
  { value: 'resolves', label: 'Until it resolves' },
];

type AlertBannersProps = {
  serverId?: string;
  checkId?: string;
  cronId?: string;
};

export function AlertBanners(props: AlertBannersProps) {
  const { serverId, checkId, cronId } = props;

  const { data: firing = [] } = useQuery(listAlertsOptions('firing'));
  const { data: mutes = [] } = useQuery(listAlertMutesOptions());
  const { data: rules = [] } = useQuery(listAlertRulesOptions());
  const [muting, setMuting] = useState<{ alert: Schemas['Alert']; isWholeRule: boolean } | undefined>();
  const [declaring, setDeclaring] = useState<DeclareDefaults | undefined>();
  const create = useCreateMute();
  const unmute = useMutation({
    ...deleteAlertMuteOptions(),
    meta: { errorMessage: 'The alert could not be unmuted' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['alert-mutes'] });
      toast.success('Unmuted');
    },
  });

  const alerts = firing.filter(
    (alert) =>
      (serverId && alert.server_id === serverId) ||
      (checkId && alert.check_id === checkId) ||
      (cronId && alert.cron_id === cronId)
  );
  if (alerts.length === 0) {
    return null;
  }

  const getTarget = (alert: Schemas['Alert']): MuteTarget => ({
    serverId: alert.server_id,
    checkId: alert.check_id,
    cronId: alert.cron_id,
    name: alert.target_name,
  });

  return (
    <>
      {alerts.map((alert) => {
        const mute = findMute(mutes, alert.rule_id, serverId ?? checkId ?? cronId);
        const detail = describeAlertNow(alert, rules.find((rule) => rule.id === alert.rule_id)?.summary);
        if (mute) {
          const resume = mute.ends_at
            ? ` If it is still firing at ${formatClock(new Date(mute.ends_at).getTime())}, the team gets a message.`
            : '';
          return (
            <div key={alert.id} className="flex items-start gap-3 rounded-xl border border-border-ring bg-surface-fill px-4 py-3.5">
              <Icon name="bell-off" className="mt-0.5 text-icon-secondary" />
              <div className="flex min-w-0 flex-col gap-0.5">
                <p className="text-body font-medium text-text-primary">
                  {alert.rule_name} is muted {describeMuteEnd(mute)}
                </p>
                <p className="text-body text-text-primary">
                  {detail} {mute.creator?.name.split(' ')[0] ?? 'Someone'} muted {isWholeRuleMute(mute) ? 'the whole rule' : `it on ${alert.target_name}`}
                  {mute.ends_at ? ` for ${formatMinutesLong(mute.created_at, mute.ends_at)}` : ' until it resolves'}
                  {mute.reason ? `: "${mute.reason}".` : '.'}
                  {resume}
                </p>
                <div className="mt-2 flex flex-wrap gap-2">
                  <Button variant="secondary" className="w-fit" onClick={() => unmute.mutate({ muteId: mute.id })}>
                    <Icon name="play" />
                    Unmute
                  </Button>
                  <Button
                    variant="secondary"
                    className="w-fit"
                    onClick={() => setMuting({ alert, isWholeRule: isWholeRuleMute(mute) })}
                  >
                    <Icon name="clock" />
                    Extend
                  </Button>
                  <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={`/alerts/${alert.rule_id}`} />}>
                    View Alert
                  </Button>
                </div>
              </div>
            </div>
          );
        }
        return (
          <div
            key={alert.id}
            className="flex items-start gap-3 rounded-xl border border-tone-critical-bg bg-tone-critical-surface px-4 py-3.5"
          >
            <Icon name="triangle-alert" className="mt-0.5 text-tone-critical-fg" />
            <div className="flex min-w-0 flex-col gap-0.5">
              <p className="text-body font-medium text-tone-critical-fg">
                {alert.rule_name} has been firing for {formatMinutesLong(alert.started_at)}
              </p>
              <p className="text-body text-text-primary">{detail}</p>
              <div className="mt-2 flex flex-wrap gap-2">
                <Button className="w-fit" onClick={() => setDeclaring(toDeclareDefaults(alert))}>
                  <Icon name="flag" />
                  Declare Incident
                </Button>
                <Menu.Root>
                  <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
                    <Icon name="bell-off" />
                    Mute
                  </Menu.Trigger>
                  <Menu.Portal>
                    <Menu.Positioner align="start">
                      <Menu.Popup>
                        <Menu.Group>
                          <Menu.GroupLabel>{alert.target_name} only</Menu.GroupLabel>
                          {QUICK_LENGTHS.map((length) => (
                            <Menu.Item
                              key={length.value}
                              onClick={() =>
                                create.mutate({
                                  rule_id: alert.rule_id,
                                  server_id: alert.server_id,
                                  check_id: alert.check_id,
                                  cron_id: alert.cron_id,
                                  ends_at: getMuteEnd(length.value),
                                  time_zone: getBrowserTimeZone(),
                                })
                              }
                            >
                              {length.label}
                            </Menu.Item>
                          ))}
                        </Menu.Group>
                        <Menu.Separator />
                        <Menu.Group>
                          <Menu.GroupLabel>Whole rule</Menu.GroupLabel>
                          <Menu.Item onClick={() => setMuting({ alert, isWholeRule: true })}>Mute {alert.rule_name}…</Menu.Item>
                        </Menu.Group>
                      </Menu.Popup>
                    </Menu.Positioner>
                  </Menu.Portal>
                </Menu.Root>
                <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={`/alerts/${alert.rule_id}`} />}>
                  View Alert
                </Button>
              </div>
            </div>
          </div>
        );
      })}
      <DeclareIncidentSheet
        open={!!declaring}
        onOpenChange={(open) => !open && setDeclaring(undefined)}
        defaults={declaring}
      />
      {muting && (
        <MuteDialog
          open
          onOpenChange={(open) => !open && setMuting(undefined)}
          ruleId={muting.alert.rule_id}
          ruleName={muting.alert.rule_name}
          ruleNoun={muting.alert.check_id ? 'check' : muting.alert.cron_id ? 'cron' : 'server'}
          resolveHint={describeResolve(rules.find((rule) => rule.id === muting.alert.rule_id)?.condition)}
          target={muting.isWholeRule ? undefined : getTarget(muting.alert)}
          isFiring
        />
      )}
    </>
  );
}

function toDeclareDefaults(alert: Schemas['Alert']): DeclareDefaults {
  const target = alert.check_id
    ? { kind: 'check', id: alert.check_id }
    : alert.cron_id
      ? { kind: 'cron', id: alert.cron_id }
      : alert.server_id
        ? { kind: 'server', id: alert.server_id }
        : undefined;
  return {
    title: `${alert.rule_name} on ${alert.target_name}`,
    severity: alert.severity === 'warning' ? 'minor' : 'major',
    components: target && target.kind !== 'server' ? [{ ...target, status: 'outage' }] : [],
    related: [{ kind: 'rule', id: alert.rule_id }, ...(target ? [target] : [])],
    alertId: alert.id,
    source: `${alert.rule_name} on ${alert.target_name}`,
  };
}

type AlertBadgeProps = {
  serverId: string;
  fallback: ReactNode;
};

export function AlertBadge(props: AlertBadgeProps) {
  const { serverId, fallback } = props;

  const { data: firing = [] } = useQuery(listAlertsOptions('firing'));
  const { data: mutes = [] } = useQuery(listAlertMutesOptions());

  const alert = firing.find((item) => item.server_id === serverId);
  if (!alert) {
    return fallback;
  }
  if (findMute(mutes, alert.rule_id, serverId)) {
    return (
      <Badge tone="neutral">
        <Icon name="bell-off" className="size-3.5" />
        {alert.rule_name} · Muted
      </Badge>
    );
  }
  return (
    <Badge tone={alert.severity === 'warning' ? 'attention' : 'warning'}>
      <Icon name="triangle-alert" className="size-3.5" />
      {alert.rule_name}
    </Badge>
  );
}
