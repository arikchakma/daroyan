import { DropdownSkeletonRow } from './dropdown-skeleton-row';

type DropdownSkeletonProps = {
  variant: 'list' | 'checklist' | 'menu';
};

export function DropdownSkeleton(props: DropdownSkeletonProps) {
  const { variant } = props;

  if (variant === 'menu') {
    return (
      <div aria-hidden className="flex flex-col">
        <DropdownSkeletonRow className="w-26" />
        <DropdownSkeletonRow className="w-20" />
        <DropdownSkeletonRow className="w-31" />
      </div>
    );
  }
  if (variant === 'checklist') {
    return (
      <div aria-hidden className="flex flex-col">
        <DropdownSkeletonRow isInset className="w-22" />
        <DropdownSkeletonRow isInset className="w-30" />
        <DropdownSkeletonRow isInset className="w-34" />
        <DropdownSkeletonRow isInset className="w-28" />
      </div>
    );
  }

  return (
    <div aria-hidden className="flex flex-col">
      <DropdownSkeletonRow className="w-33" />
      <DropdownSkeletonRow className="w-24" />
      <DropdownSkeletonRow className="w-40" />
      <DropdownSkeletonRow className="w-28" />
    </div>
  );
}
