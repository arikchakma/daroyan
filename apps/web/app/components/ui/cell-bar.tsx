import { cn } from '~/utils/classname';

type CellBarProps = {
  size: 'caption' | 'body';
  width: string;
  align?: 'left' | 'right';
  isPrimary?: boolean;
};

export function CellBar(props: CellBarProps) {
  const { size, width, align = 'left', isPrimary = false } = props;

  return (
    <span
      aria-hidden
      style={{ width }}
      className={cn(
        'flex max-w-48 shrink-0 flex-col justify-center',
        size === 'caption' && 'h-4',
        size === 'body' && 'h-5',
        align === 'right' && 'ml-auto'
      )}
    >
      <span
        className={cn(
          'w-full rounded-full bg-surface-pill',
          isPrimary ? 'h-2.5' : 'h-2'
        )}
      />
    </span>
  );
}
