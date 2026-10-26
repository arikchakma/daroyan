import { Icon } from '~/components/icons/icon';

type DropdownErrorProps = {
  plural: string;
  onRetry: () => void;
};

export function DropdownError(props: DropdownErrorProps) {
  const { plural, onRetry } = props;

  return (
    <div role="alert" className="flex h-8 shrink-0 items-center gap-2 px-2">
      <Icon name="triangle-alert" className="shrink-0 text-text-tertiary" />
      <p className="min-w-0 flex-1 truncate text-small text-text-secondary">
        Couldn’t load {plural}
      </p>
      <button
        type="button"
        onClick={onRetry}
        className="shrink-0 rounded-xs text-small font-medium text-text-primary outline-none hover:underline focus-visible:ring-2 focus-visible:ring-text-link/40"
      >
        Try Again
      </button>
    </div>
  );
}
