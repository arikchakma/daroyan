import { cn } from '~/utils/classname';

type SkeletonProps = {
  size?: 'caption' | 'body' | 'stat' | 'title' | 'box' | 'block';
  className?: string;
};

export function Skeleton(props: SkeletonProps) {
  const { size = 'body', className } = props;

  if (size === 'box' || size === 'block') {
    return (
      <span
        aria-hidden
        className={cn(
          'block shrink-0 rounded-sm',
          size === 'box' ? 'bg-surface-fill-strong' : 'bg-surface-subtle',
          className
        )}
      />
    );
  }

  return (
    <span
      aria-hidden
      className={cn(
        'flex shrink-0 flex-col justify-center',
        size === 'caption' && 'h-4',
        size === 'body' && 'h-5',
        size === 'stat' && 'h-7',
        size === 'title' && 'h-8.5',
        className
      )}
    >
      <span
        className={cn(
          'w-full rounded-full bg-surface-fill-strong',
          size === 'caption' && 'h-2',
          size === 'body' && 'h-2.5',
          size === 'stat' && 'h-3.5',
          size === 'title' && 'h-4.5'
        )}
      />
    </span>
  );
}
