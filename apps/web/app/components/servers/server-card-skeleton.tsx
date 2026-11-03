import { Skeleton } from '~/components/ui/skeleton';

export function ServerCardSkeleton() {
  return (
    <div aria-hidden className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
      <div className="flex flex-col gap-1.5 px-4 py-3.5">
        <Skeleton className="w-32" />
        <Skeleton className="w-56" />
        <Skeleton className="w-40" />
      </div>
      <div className="grid grid-cols-3 gap-px border-t border-border-row bg-border-row">
        {['CPU', 'Memory', 'Disk'].map((label) => (
          <div key={label} className="flex flex-col gap-1 bg-surface-default px-4 py-3.5">
            <Skeleton size="caption" className="w-12" />
            <Skeleton className="w-24" />
          </div>
        ))}
      </div>
    </div>
  );
}
