import { cn } from '~/utils/classname';

import { Skeleton } from './skeleton';

type DropdownSkeletonRowProps = {
  isInset?: boolean;
  className: string;
};

export function DropdownSkeletonRow(props: DropdownSkeletonRowProps) {
  const { isInset, className } = props;

  return (
    <div className={cn('flex h-8 items-center px-2', isInset && 'pl-8.5')}>
      <Skeleton className={className} />
    </div>
  );
}
