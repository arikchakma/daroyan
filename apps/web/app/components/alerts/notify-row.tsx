import type { Schemas } from '@daroyan/client';
import { useId } from 'react';

import { getIntegrationLabel } from '~/components/alerts/integration-logo';
import { PlainSelect } from '~/components/alerts/plain-select';
import { Icon } from '~/components/icons/icon';
import type { NotifyLevel } from '~/utils/alerts';
import { INTEGRATION_KINDS, isIntegrationKind, NOTIFY_OPTIONS } from '~/utils/alerts';

type NotifyRowProps = {
  integration: Schemas['Integration'];
  value: NotifyLevel;
  onValueChange: (value: NotifyLevel) => void;
};

export function NotifyRow(props: NotifyRowProps) {
  const { integration, value, onValueChange } = props;

  const titleId = useId();
  const icon = isIntegrationKind(integration.kind) ? INTEGRATION_KINDS[integration.kind].icon : 'webhook';

  return (
    <div className="flex items-center gap-3 px-3.5 py-2.5">
      <Icon name={icon} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={titleId} className="truncate text-body font-medium">
          {integration.name}
        </span>
        <span className="truncate text-body text-text-secondary">
          {getIntegrationLabel(integration.kind)} · {integration.target}
        </span>
      </div>
      <div className="w-55 shrink-0">
        <PlainSelect labelId={titleId} value={value} onValueChange={(next) => onValueChange((next ?? 'off') as NotifyLevel)} items={NOTIFY_OPTIONS} />
      </div>
    </div>
  );
}
