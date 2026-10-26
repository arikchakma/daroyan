import type { Schemas } from '@daroyan/client';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';

type ServerStatusBadgeProps = {
  status: Schemas['ServerStatus'];
};

export function ServerStatusBadge(props: ServerStatusBadgeProps) {
  const { status } = props;

  switch (status) {
    case 'healthy':
      return (
        <Badge tone="success">
          <Icon name="check" />
          Healthy
        </Badge>
      );
    case 'warning':
      return (
        <Badge tone="warning">
          <Icon name="triangle-alert" />
          Warning
        </Badge>
      );
    case 'down':
      return (
        <Badge tone="critical">
          <Icon name="circle-x" />
          Down
        </Badge>
      );
    case 'pending':
      return (
        <Badge tone="neutral">
          <Icon name="circle-dashed" />
          Pending
        </Badge>
      );
  }
}
