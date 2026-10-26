import type { ReactNode } from 'react';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Button } from '~/components/ui/button';

const ROWS = [
  { title: 'w-23', detail: 'w-33' },
  { title: 'w-17.5', detail: 'w-27.5' },
  { title: 'w-21', detail: 'w-31' },
];

type EmptyStateProps = {
  icon: IconName;
  marks: [IconName, IconName, IconName];
  title: string;
  description: string;
  children?: ReactNode;
};

export function EmptyState(props: EmptyStateProps) {
  const { icon, marks, title, description, children } = props;

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-4 pb-20 text-center">
      <div aria-hidden="true" className="relative h-42 w-70 max-w-full shrink-0 overflow-hidden rounded-[20px] bg-surface-panel">
        <div className="absolute top-7 left-8 flex h-45 w-54 flex-col gap-2 rounded-[14px] border border-border-ring bg-surface-default p-3 shadow-card">
          {ROWS.map((row, index) => (
            <div key={index} className="flex flex-col gap-2">
              {index > 0 && <span className="h-px w-full bg-surface-pill" />}
              <div className="flex items-center gap-2">
                <span className="flex size-5.5 shrink-0 items-center justify-center rounded-full bg-surface-panel text-icon-secondary [&_svg]:size-2.75">
                  <Icon name={icon} />
                </span>
                <span className="flex min-w-0 flex-1 flex-col gap-1.25">
                  <span className={`h-1.5 rounded-[3px] bg-border-strong ${row.title}`} />
                  <span className={`h-1.25 rounded-[2.5px] bg-surface-pill ${row.detail}`} />
                </span>
                <Icon name={marks[index]!} className="size-3 shrink-0 text-icon-secondary" />
              </div>
            </div>
          ))}
        </div>
      </div>
      <div className="flex flex-col items-center gap-1.5">
        <h2 className="text-title-section font-medium text-text-primary">{title}</h2>
        <p className="max-w-110 text-body text-balance text-text-secondary">{description}</p>
      </div>
      {children && <div className="flex flex-wrap justify-center gap-2">{children}</div>}
    </div>
  );
}

type NoResultsProps = {
  title: string;
  onReset: () => void;
};

export function NoResults(props: NoResultsProps) {
  const { title, onReset } = props;

  return (
    <div className="flex flex-col items-center gap-1 py-12 text-center">
      <Icon name="search" className="mb-2 size-5 text-icon-secondary" />
      <p className="text-body font-medium text-text-primary">{title}</p>
      <p className="text-body text-text-secondary">Try changing the view or the search term.</p>
      <Button variant="secondary" className="mt-3 w-fit" onClick={onReset}>
        Reset View
      </Button>
    </div>
  );
}
