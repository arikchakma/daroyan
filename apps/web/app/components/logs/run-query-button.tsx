import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { cn } from '~/utils/classname';
import { formatHotkey, isMacPlatform } from '~/utils/hotkey';

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
      title={`Run query (${formatHotkey('Mod+Enter')})`}
      aria-keyshortcuts={isMacPlatform() ? 'Meta+Enter' : 'Control+Enter'}
      className="shrink-0 gap-1.5 rounded-sm pr-1"
      onClick={onRun}
    >
      <Icon name={isRunning ? 'loader' : 'play'} className={cn('size-3!', isRunning && 'animate-spin')} />
      Run
      <span aria-hidden="true" className="flex h-5 items-center gap-0.5 rounded-xs bg-current/15 px-1 font-sans text-axis">
        {isMacPlatform() ? <Icon name="command" className="size-3!" /> : 'Ctrl'}
        <Icon name="corner-down-left" className="size-3!" />
      </span>
    </Button>
  );
}
