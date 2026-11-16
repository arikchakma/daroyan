import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { cn } from '~/utils/classname';

type RunQueryButtonProps = {
  isChanged: boolean;
  isRunning: boolean;
  onRun: () => void;
};

export function RunQueryButton(props: RunQueryButtonProps) {
  const { isChanged, isRunning, onRun } = props;

  return (
    <Button
      type="button"
      variant={isChanged ? 'primary' : 'tertiary'}
      size="xs"
      title="Run query"
      disabled={isRunning}
      className="shrink-0 gap-1.5 rounded-sm pr-1"
      onClick={onRun}
    >
      <Icon name={isRunning ? 'loader' : 'play'} className={cn('size-3!', isRunning && 'animate-spin')} />
      Run
      <span className="flex size-5 items-center justify-center rounded-xs bg-current/15">
        <Icon name="corner-down-left" className="size-3!" />
      </span>
    </Button>
  );
}
