import { useContext } from 'react';

import { Icon } from '~/components/icons/icon';
import { LogActions } from '~/components/logs/log-actions';
import { LogBody } from '~/components/logs/log-body';
import { PanelContext } from '~/components/navigation/panel-context';
import { Button } from '~/components/ui/button';
import { Sheet } from '~/components/ui/sheet';
import type { LogRow } from '~/hooks/use-logs-table';
import { cn } from '~/utils/classname';
import { formatLogClock } from '~/utils/format';
import type { QueryTerm } from '~/utils/log-search';

type LogSheetProps = {
  line?: LogRow;
  canGoPrevious: boolean;
  canGoNext: boolean;
  onNavigate: (step: -1 | 1) => void;
  onClose: () => void;
  onFilter?: (term: QueryTerm) => void;
};

export function LogSheet(props: LogSheetProps) {
  const { line, canGoPrevious, canGoNext, onNavigate, onClose, onFilter } = props;

  const panel = useContext(PanelContext);

  return (
    <Sheet.Root side="bottom" open={!!line} onOpenChange={(open) => !open && onClose()}>
      <Sheet.Portal container={panel}>
        <Sheet.Backdrop className="bg-black opacity-[calc(0.4*(1-var(--drawer-swipe-progress)))]" />
        <Sheet.Viewport className={cn('pt-4 pb-1', panel ? 'absolute pr-1' : 'fixed px-1')}>
          <Sheet.Popup className="h-auto flex-1 gap-0 rounded-2xl border-0 bg-surface-page text-text-primary shadow-popover ring-1 ring-border-elevated transition-[translate,opacity] ring-inset data-ending-style:translate-y-4 data-ending-style:opacity-0 data-starting-style:translate-y-4 data-starting-style:opacity-0">
            <div className="bar-stuck grid h-14 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 px-4">
              <div className="flex min-w-0 items-center gap-2">
                <Icon name="terminal" className="shrink-0" />
                <Sheet.Title className="truncate font-mono text-code font-normal">
                  {line ? `${line.service} · ${formatLogClock(line.at)}` : 'Log Line'}
                </Sheet.Title>
              </div>
              <div className="flex items-center gap-1 rounded-full bg-surface-code p-1 empty:hidden">
                {line && <LogActions key={line.id} line={line} />}
              </div>
              <div className="flex items-center justify-end gap-2">
                <div className="flex">
                  <Button
                    variant="secondary"
                    size="icon"
                    aria-label="Next Line"
                    disabled={!canGoNext}
                    onClick={() => onNavigate(1)}
                  >
                    <Icon name="chevron-down" />
                  </Button>
                  <Button
                    variant="secondary"
                    size="icon"
                    aria-label="Previous Line"
                    disabled={!canGoPrevious}
                    onClick={() => onNavigate(-1)}
                  >
                    <Icon name="chevron-up" />
                  </Button>
                </div>
                <Sheet.Close aria-label="Close" render={<Button variant="secondary" size="icon" />}>
                  <Icon name="x" />
                </Sheet.Close>
              </div>
            </div>
            {line && <LogBody key={line.id} line={line} onFilter={onFilter} />}
          </Sheet.Popup>
        </Sheet.Viewport>
      </Sheet.Portal>
    </Sheet.Root>
  );
}
