import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';

type UptimeStatusBadgeProps = {
  status: string;
};

export function UptimeStatusBadge(props: UptimeStatusBadgeProps) {
  const { status } = props;

  switch (status) {
    case 'up':
      return (
        <Badge tone="success">
          <Icon name="check" />
          Up
        </Badge>
      );
    case 'degraded':
      return (
        <Badge tone="warning">
          <Icon name="triangle-alert" />
          Degraded
        </Badge>
      );
    case 'down':
      return (
        <Badge tone="critical">
          <Icon name="circle-x" />
          Down
        </Badge>
      );
    case 'paused':
      return (
        <Badge tone="neutral">
          <Icon name="pause" />
          Paused
        </Badge>
      );
    default:
      return (
        <Badge tone="neutral">
          <Icon name="circle-dashed" />
          Pending
        </Badge>
      );
  }
}
