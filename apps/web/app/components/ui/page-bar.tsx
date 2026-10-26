import type { ReactNode } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { Button } from '~/components/ui/button';
import { cn } from '~/utils/classname';

type PageBarProps = {
  icon: IconName;
  title: string;
  backTo?: string;
  children?: ReactNode;
};

export function PageBar(props: PageBarProps) {
  const { icon, title, backTo, children } = props;

  const Title = backTo ? 'span' : 'h1';

  return (
    <header
      className={cn(
        'header-stuck flex shrink-0 items-center pt-3 pr-3 pb-2.5',
        backTo ? 'gap-2.5 pl-3.5' : 'gap-2 pl-4'
      )}
    >
      {backTo && (
        <Button
          nativeButton={false}
          render={<Link to={backTo} aria-label={`Back to ${title}`} />}
          variant="secondary"
          size="icon"
          className="size-7.5"
        >
          <Icon name="chevron-left" />
        </Button>
      )}
      <Title className="flex min-w-0 flex-1 items-center gap-2 text-body font-medium">
        <Icon name={icon} className="shrink-0" />
        <span className="truncate">{title}</span>
      </Title>
      {children}
    </header>
  );
}
