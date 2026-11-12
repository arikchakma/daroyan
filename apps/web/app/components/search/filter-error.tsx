import { Icon } from '~/components/icons/icon';

type FilterErrorProps = {
  message: string;
};

export function FilterError(props: FilterErrorProps) {
  const { message } = props;

  return (
    <p role="alert" className="flex h-7 shrink-0 items-center gap-2 px-2 text-body text-tone-critical-fg">
      <Icon name="triangle-alert" className="shrink-0" />
      <span className="min-w-0 flex-1 truncate">{message}</span>
    </p>
  );
}
