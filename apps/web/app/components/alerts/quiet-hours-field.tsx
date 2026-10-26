import type { Schemas } from '@daroyan/client';
import { useId } from 'react';

import { SelectField } from '~/components/alerts/rule-form';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { TimePicker } from '~/components/ui/time-picker';
import { WEEKDAYS } from '~/utils/alerts';
import { cn } from '~/utils/classname';

type QuietWindow = Schemas['QuietWindow'];

function getOffset(zone: string) {
  const name =
    new Intl.DateTimeFormat('en-US', { timeZone: zone, timeZoneName: 'longOffset' })
      .formatToParts(new Date())
      .find((part) => part.type === 'timeZoneName')?.value ?? 'GMT';
  return name === 'GMT' ? 'GMT+00:00' : name;
}

function getCity(zone: string) {
  return (zone.split('/').at(-1) ?? zone).replaceAll('_', ' ');
}

let timeZones: { value: string; label: string }[] | undefined;

export function getTimeZones() {
  timeZones ??= Intl.supportedValuesOf('timeZone')
    .map((zone) => ({ zone, offset: getOffset(zone) }))
    .sort((a, b) => {
      const minutes = (offset: string) => {
        const match = offset.match(/([+-])(\d{2}):(\d{2})/);
        return match ? (match[1] === '-' ? -1 : 1) * (Number(match[2]) * 60 + Number(match[3])) : 0;
      };
      return minutes(a.offset) - minutes(b.offset) || a.zone.localeCompare(b.zone);
    })
    .map(({ zone, offset }) => ({ value: zone, label: `(${offset}) ${getCity(zone)}` }));
  return timeZones;
}

export const DEFAULT_QUIET_WINDOW: QuietWindow = { days: [0, 1, 2, 3, 4], from: '02:00', to: '03:00' };

export function getBrowserTimeZone() {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

function describeDays(days: number[]) {
  const sorted = [...days].sort();
  if (sorted.length === 7) {
    return 'Daily';
  }
  if (sorted.join() === '0,1,2,3,4') {
    return 'Weekdays';
  }
  if (sorted.join() === '5,6') {
    return 'Weekends';
  }
  return sorted.map((day) => WEEKDAYS[day]).join(', ');
}

export function describeQuietWindows(windows: QuietWindow[], timeZone: string) {
  const parts = windows.map((window) => `${describeDays(window.days)}, ${window.from} to ${window.to}`);
  return `${parts.join('; ')} ${getCity(timeZone)} time`;
}

type QuietHoursFieldProps = {
  windows: QuietWindow[];
  timeZone: string;
  onWindowsChange: (windows: QuietWindow[]) => void;
  onTimeZoneChange: (timeZone: string) => void;
};

export function QuietHoursField(props: QuietHoursFieldProps) {
  const { windows, timeZone, onWindowsChange, onTimeZoneChange } = props;

  const change = (index: number, next: Partial<QuietWindow>) =>
    onWindowsChange(windows.map((window, at) => (at === index ? { ...window, ...next } : window)));

  return (
    <div className="flex flex-col gap-3.5">
      {windows.map((window, index) => (
        <QuietWindowFields
          key={index}
          window={window}
          timeZone={index === 0 ? timeZone : undefined}
          onChange={(next) => change(index, next)}
          onTimeZoneChange={onTimeZoneChange}
          onRemove={index > 0 ? () => onWindowsChange(windows.filter((_, at) => at !== index)) : undefined}
        />
      ))}
      <p className="text-body text-text-secondary">
        {describeQuietWindows(windows, timeZone)}. Alerts still fire and show in Daroyan, but no messages go out.
      </p>
      <div className="flex gap-4">
        {windows.length < 5 && (
          <Button
            type="button"
            variant="plain"
            className="w-fit"
            onClick={() => onWindowsChange([...windows, DEFAULT_QUIET_WINDOW])}
          >
            <Icon name="plus" />
            Add Another Window
          </Button>
        )}
        <Button type="button" variant="plain" className="w-fit" onClick={() => onWindowsChange([])}>
          <Icon name="trash" />
          Remove Quiet Hours
        </Button>
      </div>
    </div>
  );
}

type QuietWindowFieldsProps = {
  window: QuietWindow;
  timeZone?: string;
  onChange: (next: Partial<QuietWindow>) => void;
  onTimeZoneChange: (timeZone: string) => void;
  onRemove?: () => void;
};

function QuietWindowFields(props: QuietWindowFieldsProps) {
  const { window, timeZone, onChange, onTimeZoneChange, onRemove } = props;

  const fromId = useId();
  const toId = useId();

  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex flex-col gap-1.5">
        <span className="text-body font-medium">Days</span>
        <div className="flex flex-wrap gap-1">
          {WEEKDAYS.map((day, dayIndex) => {
            const isOn = window.days.includes(dayIndex);
            return (
              <button
                key={day}
                type="button"
                aria-pressed={isOn}
                onClick={() =>
                  onChange({ days: isOn ? window.days.filter((item) => item !== dayIndex) : [...window.days, dayIndex] })
                }
                className={cn(
                  'h-7 rounded-md px-2.5 text-body outline-none focus-visible:ring-2 focus-visible:ring-text-link/40',
                  isOn ? 'bg-surface-fill text-text-primary' : 'text-text-secondary hover:bg-surface-subtle'
                )}
              >
                {day}
              </button>
            );
          })}
        </div>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-[150px_150px_1fr]">
        <div className="flex flex-col gap-1.5">
          <label htmlFor={fromId} className="text-body font-medium">
            From
          </label>
          <TimePicker id={fromId} value={window.from} onValueChange={(from) => onChange({ from })} />
        </div>
        <div className="flex flex-col gap-1.5">
          <label htmlFor={toId} className="text-body font-medium">
            To
          </label>
          <TimePicker id={toId} value={window.to} onValueChange={(to) => onChange({ to })} />
        </div>
        {timeZone ? (
          <SelectField
            label="Time Zone"
            value={timeZone}
            onValueChange={(value) => onTimeZoneChange(value ?? timeZone)}
            items={getTimeZones()}
          />
        ) : (
          <div className="flex items-end">
            <Button type="button" variant="plain" className="w-fit" onClick={onRemove}>
              <Icon name="trash" />
              Remove Window
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
