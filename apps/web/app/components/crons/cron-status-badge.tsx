import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';

type CronStatusBadgeProps = {
  status: string;
};

export function CronStatusBadge(props: CronStatusBadgeProps) {
  const { status } = props;

  switch (status) {
    case 'ok':
      return (
        <Badge tone="success">
          <Icon name="check" />
          OK
        </Badge>
      );
    case 'late':
      return (
        <Badge tone="attention">
          <Icon name="clock" />
          Late
        </Badge>
      );
    case 'failed':
      return (
        <Badge tone="critical">
          <Icon name="circle-x" />
          Failed
        </Badge>
      );
    case 'missed':
      return (
        <Badge tone="critical">
          <Icon name="circle-dashed" />
          Missed
        </Badge>
      );
    case 'running':
      return (
        <Badge tone="info">
          <Icon name="loader" />
          Running
        </Badge>
      );
    case 'paused':
      return (
        <Badge tone="neutral">
          <Icon name="pause" />
          Paused
        </Badge>
      );
    case 'muted':
      return (
        <Badge tone="neutral">
          <Icon name="bell-off" />
          Muted
        </Badge>
      );
    default:
      return <Badge tone="info">New</Badge>;
  }
}
