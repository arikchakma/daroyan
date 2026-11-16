import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';

type RunQueryButtonProps = {
  isChanged: boolean;
  isRunning: boolean;
  onRun: () => void;
};

export function RunQueryButton(props: RunQueryButtonProps) {
  const { isChanged, isRunning, onRun } = props;

  if (isRunning) {
    return (
      <Button type="button" variant="primary" size="xs" className="shrink-0 rounded-sm" disabled>
        <Icon name="loader" className="animate-spin" />
        Running
      </Button>
    );
  }

  return (
    <Button
      type="button"
      variant={isChanged ? 'primary' : 'tertiary'}
      size="xs"
      title="Run query"
      className="shrink-0 gap-1.5 rounded-sm pr-1"
      onClick={onRun}
    >
      <Icon name="play" className="size-3!" />
      Run
      <span className="flex size-5 items-center justify-center rounded-xs bg-current/15">
        <Icon name="corner-down-left" className="size-3!" />
      </span>
    </Button>
  );
}
