import type { ReactNode } from 'react';
import { useId } from 'react';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';

type SettingsPageProps = {
  title: string;
  description?: string;
  icon: IconName;
  actions?: ReactNode;
  width?: 'default' | 'wide';
  children: ReactNode;
};

export function SettingsPage(props: SettingsPageProps) {
  const {
    title,
    description,
    icon,
    actions,
    width = 'default',
    children,
  } = props;

  return (
    <div className="flex min-h-full flex-col bg-surface-default text-text-primary">
      <header className="header-stuck flex shrink-0 items-center gap-2 pt-3 pr-3 pb-2.5 pl-4">
        <div
          aria-hidden="true"
          className="flex header-title items-center gap-2"
        >
          <Icon name={icon} />
          <span className="text-body font-medium">{title}</span>
        </div>
        {actions && (
          <div className="ml-auto flex items-center gap-2">{actions}</div>
        )}
      </header>
      <div className="-mt-header flex justify-center px-4 pt-12 pb-16">
        <div
          className={cn(
            'flex max-w-full flex-col gap-8',
            width === 'default' && 'w-160',
            width === 'wide' && 'w-235'
          )}
        >
          <div className="flex flex-col gap-1.5">
            <h1 className="text-greeting font-medium">{title}</h1>
            {description && (
              <p className="text-body text-text-secondary">{description}</p>
            )}
          </div>
          {children}
        </div>
      </div>
    </div>
  );
}

type SettingsBlockProps = {
  title: string;
  description?: string;
  info?: string;
  children: ReactNode;
};

export function SettingsBlock(props: SettingsBlockProps) {
  const { title, description, info, children } = props;

  const titleId = useId();

  return (
    <section aria-labelledby={titleId} className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <div className="flex items-center gap-1.5">
          <h2 id={titleId} className="text-title-card font-medium">
            {title}
          </h2>
          {info && (
            <Tooltip.Root>
              <Tooltip.Trigger
                delay={200}
                aria-label={info}
                className="flex rounded-xs text-text-tertiary outline-none focus-visible:ring-2 focus-visible:ring-text-link/40"
              >
                <Icon name="info" className="size-3.5" />
              </Tooltip.Trigger>
              <Tooltip.Portal>
                <Tooltip.Positioner side="top" sideOffset={6}>
                  <Tooltip.Popup className="w-64 rounded-lg border border-border-ring bg-surface-default p-3 text-small font-normal text-text-secondary shadow-popover">
                    {info}
                  </Tooltip.Popup>
                </Tooltip.Positioner>
              </Tooltip.Portal>
            </Tooltip.Root>
          )}
        </div>
        {description && (
          <p className="text-body text-text-secondary">{description}</p>
        )}
      </div>
      {children}
    </section>
  );
}

type SettingsCardProps = {
  className?: string;
  children: ReactNode;
};

export function SettingsCard(props: SettingsCardProps) {
  const { className, children } = props;

  return (
    <div
      className={cn(
        'flex flex-col overflow-clip rounded-2xl border border-border-ring bg-surface-default shadow-card',
        className
      )}
    >
      {children}
    </div>
  );
}
