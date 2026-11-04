import type { Schemas } from '@daroyan/client';

import { OverviewRow } from '~/components/overview/overview-row';
import { DetailSection } from '~/components/servers/detail-table';
import { getUpcomingDate, getUpcomingHref, getUpcomingIcon } from '~/utils/overview';

type ComingUpProps = {
  items: Schemas['UpcomingItem'][];
};

export function ComingUp(props: ComingUpProps) {
  const { items } = props;

  return (
    <DetailSection title="Coming Up" summary="Next 30 days">
      <div className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        {items.map((item) => (
          <OverviewRow
            key={`${item.kind}-${item.target_id ?? ''}-${item.detail}`}
            href={getUpcomingHref(item)}
            icon={getUpcomingIcon(item.kind)}
            tone={item.soon ? 'warning' : 'neutral'}
            title={item.title}
            detail={item.detail}
            date={getUpcomingDate(item)}
          />
        ))}
      </div>
    </DetailSection>
  );
}
