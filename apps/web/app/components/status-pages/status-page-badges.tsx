import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { getPageStatus } from '~/utils/status-pages';

type PageStatusBadgeProps = {
  status: string;
};

export function PageStatusBadge(props: PageStatusBadgeProps) {
  const { status } = props;

  const { tone, icon, label } = getPageStatus(status);

  return (
    <Badge tone={tone}>
      <Icon name={icon} />
      {label}
    </Badge>
  );
}

type VisibilityBadgeProps = {
  isPublic: boolean;
};

export function VisibilityBadge(props: VisibilityBadgeProps) {
  const { isPublic } = props;

  return (
    <Badge>
      <Icon name={isPublic ? 'globe' : 'lock'} />
      {isPublic ? 'Public' : 'Private'}
    </Badge>
  );
}
