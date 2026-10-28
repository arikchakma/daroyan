import type { Schemas } from '@daroyan/client';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { describeFiring, describeMuteEnd, getAlertTargetPath, joinNames } from '~/utils/alerts';
import { cn } from '~/utils/classname';
import { formatClock } from '~/utils/format';

type FiringRowProps = {
  alert: Schemas['Alert'];
  deliveries: Schemas['AlertDelivery'][];
  mute?: Schemas['AlertMute'];
  onMute: () => void;
  onUnmute: (muteId: string) => void;
};

export function FiringRow(props: FiringRowProps) {
  const { alert, deliveries, mute, onMute, onUnmute } = props;

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
            <Icon name={alert.check_id ? 'activity' : alert.cron_id ? 'clock' : 'server'} />
            {alert.check_id ? 'Open Check' : alert.cron_id ? 'Open Cron' : 'Open Server'}
          </Button>
        </>
      )}
    </div>
  );
}
