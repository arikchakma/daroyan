import type { Schemas } from '@daroyan/client';

import { Icon } from '~/components/icons/icon';
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
        {items.length === 0 ? (
          <div className="flex gap-3 px-4 py-3">
            <Icon name="circle-check" className="mt-0.5 text-text-secondary" />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5 text-body">
              <span className="font-medium">Nothing coming up</span>
              <span className="truncate text-text-secondary">
                No disks filling, certificates expiring or maintenance in the next 30 days.
              </span>
            </div>
          </div>
        ) : (
          items.map((item) => (
            <OverviewRow
              key={`${item.kind}-${item.target_id ?? ''}-${item.detail}`}
              href={getUpcomingHref(item)}
              icon={getUpcomingIcon(item.kind)}
              tone={item.soon ? 'warning' : 'neutral'}
              title={item.title}
              detail={item.detail}
              date={getUpcomingDate(item)}
            />
          ))
        )}
      </div>
    </DetailSection>
  );
}
