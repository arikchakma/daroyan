import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';

export type RuleState = 'firing' | 'muted' | 'limited' | 'ok' | 'disabled';

type RuleStatusBadgeProps = {
  state: RuleState;
  label?: string;
};

export function RuleStatusBadge(props: RuleStatusBadgeProps) {
  const { state, label } = props;

  if (state === 'firing') {
    return (
      <Badge tone="critical">
        <Icon name="triangle-alert" className="size-3.5" />
        {label ?? 'Firing'}
      </Badge>
    );
  }
  if (state === 'muted') {
    return (
      <Badge tone="neutral">
        <Icon name="bell-off" className="size-3.5" />
        {label ?? 'Muted'}
      </Badge>
    );
  }
  if (state === 'limited') {
    return (
      <Badge tone="warning">
        <Icon name="clock" className="size-3.5" />
        {label ?? 'Limit reached'}
      </Badge>
    );
  }
  if (state === 'ok') {
    return (
      <Badge tone="success">
        <Icon name="check" className="size-3.5" />
        OK
      </Badge>
    );
  }
  return <Badge tone="neutral">Disabled</Badge>;
}
