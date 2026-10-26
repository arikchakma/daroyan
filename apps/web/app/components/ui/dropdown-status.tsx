import { DropdownError } from './dropdown-error';
import { DropdownNote } from './dropdown-note';
import { DropdownSkeleton } from './dropdown-skeleton';

type DropdownStatusProps = {
  plural: string;
  search?: string;
  variant: 'list' | 'checklist' | 'menu';
  isLoading: boolean;
  isFailed: boolean;
  isEmpty: boolean;
  onRetry: () => void;
};

export function DropdownStatus(props: DropdownStatusProps) {
  const { plural, search, variant, isLoading, isFailed, isEmpty, onRetry } =
    props;

  if (isLoading) {
    return <DropdownSkeleton variant={variant} />;
  }
  if (isFailed) {
    return <DropdownError plural={plural} onRetry={onRetry} />;
  }
  if (!isEmpty) {
    return null;
  }
  if (search) {
    return (
      <DropdownNote>
        No {plural} match “{search}”.
      </DropdownNote>
    );
  }

  return <DropdownNote>No {plural} yet.</DropdownNote>;
}
