import type { Schemas } from '@daroyan/client';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Button } from '~/components/ui/button';
import { describeFiring, describeMuteEnd, getAlertTargetPath, joinNames } from '~/utils/alerts';
import { cn } from '~/utils/classname';
import { formatClock } from '~/utils/format';
import { OTLP_SERVER_ID } from '~/utils/logs';

function getOpenAction(alert: Schemas['Alert']): { icon: IconName; label: string } {
  if (alert.check_id) {
    return { icon: 'activity', label: 'Open Check' };
  }
  if (alert.cron_id) {
    return { icon: 'clock', label: 'Open Cron' };
  }
  if (alert.server_id === OTLP_SERVER_ID) {
    return { icon: 'terminal', label: 'Open Logs' };
  }
  return { icon: 'server', label: 'Open Server' };
}

type FiringRowProps = {
  alert: Schemas['Alert'];
  deliveries: Schemas['AlertDelivery'][];
  mute?: Schemas['AlertMute'];
  onMute: () => void;
  onUnmute: (muteId: string) => void;
};

export function FiringRow(props: FiringRowProps) {
  const { alert, deliveries, mute, onMute, onUnmute } = props;

  const openAction = getOpenAction(alert);
  const names = [...new Set(deliveries.map((delivery) => delivery.integration_name))];
  const sentAt = deliveries.at(-1)?.sent_at;
  const sent = mute
    ? ` Muted by ${mute.creator?.name.split(' ')[0] ?? 'Someone'} ${describeMuteEnd(mute)}${mute.reason ? `: "${mute.reason}"` : ''}.`
    : names.length > 0 && sentAt
      ? ` Sent to ${joinNames(names)} at ${formatClock(new Date(sentAt).getTime())}.`
      : '';

  return (
    <div className="flex items-center gap-3 py-3 pr-3 pl-4">
      <span
        className={cn(
          'size-1.5 shrink-0 rounded-full bg-tone-critical-fg',
          alert.severity === 'warning' && 'bg-toast-warning-icon',
          mute && 'bg-icon-secondary'
        )}
      />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
        <span className="truncate font-medium">
          {alert.target_name}
          {alert.severity === 'warning' && <span className="font-normal text-text-tertiary"> · Warning</span>}
        </span>
        <span className="text-text-secondary">
          {describeFiring(alert)}.{sent}
        </span>
      </div>
      {mute ? (
        <Button variant="secondary" className="w-fit" onClick={() => onUnmute(mute.id)}>
          <Icon name="play" />
          Unmute
        </Button>
      ) : (
        <>
          <Button variant="secondary" className="w-fit" onClick={onMute}>
            <Icon name="bell-off" />
            Mute
          </Button>
          <Button
            nativeButton={false}
            render={<Link to={getAlertTargetPath(alert)} />}
            variant="secondary"
            className="w-fit"
          >
            <Icon name={openAction.icon} />
            {openAction.label}
          </Button>
        </>
      )}
    </div>
  );
}
