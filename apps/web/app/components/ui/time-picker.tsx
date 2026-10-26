import { useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { Popover } from '~/components/ui/popover';
import { cn } from '~/utils/classname';

const TIMES = Array.from(
  { length: 48 },
  (_, index) => `${String(Math.floor(index / 2)).padStart(2, '0')}:${index % 2 ? '30' : '00'}`
);

export function readTime(text: string) {
  const match = text.trim().toLowerCase().match(/^(\d{1,2})(?::?(\d{2}))?\s*(am|pm)?$/);
  if (!match) {
    return undefined;
  }
  let hours = Number(match[1]);
  const minutes = Number(match[2] ?? 0);
  if (match[3] === 'pm' && hours < 12) {
    hours += 12;
  }
  if (match[3] === 'am' && hours === 12) {
    hours = 0;
  }
  if (hours > 23 || minutes > 59) {
    return undefined;
  }
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}`;
}

type TimePickerProps = {
  id?: string;
  value: string;
  onValueChange: (value: string) => void;
};

export function TimePicker(props: TimePickerProps) {
  const { id, value, onValueChange } = props;

  const [isOpen, setIsOpen] = useState(false);
  const [typed, setTyped] = useState('');

  const pick = (time: string) => {
    onValueChange(time);
    setIsOpen(false);
  };

  return (
    <Popover.Root
      open={isOpen}
      onOpenChange={(open) => {
        setTyped('');
        setIsOpen(open);
      }}
    >
      <Popover.Trigger
        id={id}
        className="flex h-9 w-full cursor-default items-center gap-2 rounded-lg border border-border-ring bg-surface-default pr-2.5 pl-3 text-left text-body text-text-primary outline-none focus-visible:border-border-strong data-popup-open:border-border-strong"
      >
        <span className="min-w-0 flex-1 truncate tabular-nums">{value}</span>
        <Icon name="chevrons-updown" className="shrink-0 text-icon-secondary" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="start" sideOffset={4}>
          <Popover.Popup className="flex max-h-72 w-(--anchor-width) min-w-28 flex-col gap-px overflow-y-auto p-1">
            <input
              autoFocus
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              onKeyDown={(event) => {
                const time = readTime(typed);
                if (event.key === 'Enter' && time) {
                  event.preventDefault();
                  pick(time);
                }
              }}
              placeholder="Type a time, like 2:15"
              aria-label="Type a time"
              className="h-7 shrink-0 bg-transparent px-2 text-caption text-text-primary outline-none placeholder:text-text-tertiary"
            />
            {TIMES.map((time) => (
              <button
                key={time}
                type="button"
                onClick={() => pick(time)}
                ref={(node) => {
                  if (node && time === value) {
                    node.scrollIntoView({ block: 'nearest' });
                  }
                }}
                className={cn(
                  'flex h-8 shrink-0 items-center gap-2.5 rounded-sm px-2 text-left text-body tabular-nums outline-none hover:bg-surface-fill focus-visible:bg-surface-fill',
                  time === value && 'bg-surface-fill'
                )}
              >
                <span className="flex-1">{time}</span>
                {time === value && <Icon name="check" />}
              </button>
            ))}
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
