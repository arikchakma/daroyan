import { useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Popover } from '~/components/ui/popover';
import { cn } from '~/utils/classname';

const WEEKDAYS = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];
const MONTH_FORMAT = new Intl.DateTimeFormat('en-US', { month: 'long', year: 'numeric' });
const DAY_FORMAT = new Intl.DateTimeFormat('en-US', { weekday: 'short', month: 'short', day: 'numeric', year: 'numeric' });

export function toDayKey(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function fromDayKey(day: string) {
  const [year = 0, month = 1, date = 1] = day.split('-').map(Number);
  return new Date(year, month - 1, date);
}

function getMonthCells(year: number, month: number) {
  const offset = (new Date(year, month, 1).getDay() + 6) % 7;
  const days = new Date(year, month + 1, 0).getDate();
  const cells: (string | null)[] = [
    ...Array.from({ length: offset }, () => null),
    ...Array.from({ length: days }, (_, index) => toDayKey(new Date(year, month, index + 1))),
  ];
  while (cells.length % 7 !== 0) {
    cells.push(null);
  }
  return cells;
}

type DatePickerProps = {
  id?: string;
  value: string;
  onValueChange: (value: string) => void;
  min?: string;
};

export function DatePicker(props: DatePickerProps) {
  const { id, value, onValueChange, min } = props;

  const [isOpen, setIsOpen] = useState(false);
  const [month, setMonth] = useState(() => {
    const date = fromDayKey(value);
    return { year: date.getFullYear(), month: date.getMonth() };
  });
  const today = toDayKey(new Date());
  const shift = (months: number) => {
    const date = new Date(month.year, month.month + months, 1);
    setMonth({ year: date.getFullYear(), month: date.getMonth() });
  };

  return (
    <Popover.Root open={isOpen} onOpenChange={setIsOpen}>
      <Popover.Trigger
        id={id}
        className="flex h-9 w-full cursor-default items-center gap-2 rounded-lg border border-border-ring bg-surface-default pr-2.5 pl-3 text-left text-body text-text-primary outline-none focus-visible:border-border-strong data-popup-open:border-border-strong"
      >
        <span className="min-w-0 flex-1 truncate">{DAY_FORMAT.format(fromDayKey(value))}</span>
        <Icon name="chevrons-updown" className="shrink-0 text-icon-secondary" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="start" sideOffset={6}>
          <Popover.Popup className="flex w-auto flex-col gap-1.5 px-3 py-2.5">
            <div className="flex h-7 items-center">
              <Button variant="plain" size="icon" aria-label="Previous month" onClick={() => shift(-1)}>
                <Icon name="chevron-left" />
              </Button>
              <span className="flex-1 text-center text-body font-medium">
                {MONTH_FORMAT.format(new Date(month.year, month.month, 1))}
              </span>
              <Button variant="plain" size="icon" aria-label="Next month" onClick={() => shift(1)}>
                <Icon name="chevron-right" />
              </Button>
            </div>
            <div className="grid w-61.5 grid-cols-7">
              {WEEKDAYS.map((weekday) => (
                <span key={weekday} className="text-center text-caption text-text-tertiary">
                  {weekday}
                </span>
              ))}
              {getMonthCells(month.year, month.month).map((day, index) =>
                day ? (
                  <button
                    key={day}
                    type="button"
                    aria-pressed={day === value}
                    disabled={!!min && day < min}
                    onClick={() => {
                      onValueChange(day);
                      setIsOpen(false);
                    }}
                    className={cn(
                      'size-8.5 rounded-md text-body outline-none hover:bg-surface-fill-strong focus-visible:ring-2 focus-visible:ring-text-link/40 disabled:pointer-events-none disabled:text-text-tertiary',
                      day === today && 'border border-border-ring',
                      day === value && 'border-0 bg-button-primary text-text-on-primary hover:bg-button-primary'
                    )}
                  >
                    {fromDayKey(day).getDate()}
                  </button>
                ) : (
                  <span key={`empty-${index}`} className="h-8.5" />
                )
              )}
            </div>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
