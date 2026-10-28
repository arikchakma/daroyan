import type { Schemas } from '@daroyan/client';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { formatRelative } from '~/utils/format';

type DeliveryStatusProps = {
  integration: Schemas['Integration'];
};
export function DeliveryStatus(props: DeliveryStatusProps) {
  const { integration } = props;

  if (!integration.last_sent_at) {
    return <span className="shrink-0 text-body text-text-secondary">No deliveries yet</span>;
  }
  if (integration.last_status !== 'failed') {
    return (
      <span className="shrink-0 text-body text-text-secondary">
        Delivered {formatRelative(integration.last_sent_at)}
      </span>
    );
  }
  return (
    <div className="flex max-w-70 min-w-0 shrink flex-col items-end gap-0.5">
      <div className="flex items-center gap-1.5">
        <Badge tone="critical">
          <Icon name="triangle-alert" className="size-3.5" />
          Failed
        </Badge>
        <span className="text-body whitespace-nowrap text-text-secondary">{formatRelative(integration.last_sent_at)}</span>
      </div>
      {integration.last_error && (
        <span className="max-w-full truncate text-body text-tone-critical-fg" title={integration.last_error}>
          {integration.last_error}
        </span>
      )}
    </div>
  );
}
