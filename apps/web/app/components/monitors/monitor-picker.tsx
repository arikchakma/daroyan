import { useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Checkbox } from '~/components/ui/checkbox';
import { Popover } from '~/components/ui/popover';
import { MONITOR_GROUPS, getMonitorKey } from '~/utils/status-pages';
import type { Monitor } from '~/utils/status-pages';

type MonitorPickerProps = {
  label: string;
  monitors: Monitor[];
  selected: string[];
  describeCount: (count: number) => string;
  describeMonitor?: (key: string) => string | undefined;
  groups?: { kind: string; label: string }[];
  searchLabel?: string;
  onApply: (keys: string[]) => void;
};

export function MonitorPicker(props: MonitorPickerProps) {
  const {
    label,
    monitors,
    selected,
    describeCount,
    describeMonitor,
    groups = MONITOR_GROUPS,
    searchLabel = 'Search checks and crons',
    onApply,
  } = props;

  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [picked, setPicked] = useState<string[]>([]);

  const term = search.trim().toLowerCase();
  const visible = monitors.filter(
    (monitor) => !term || monitor.name.toLowerCase().includes(term) || monitor.caption.toLowerCase().includes(term)
  );

  const toggle = (key: string) =>
    setPicked((current) => (current.includes(key) ? current.filter((other) => other !== key) : [...current, key]));

  return (
    <Popover.Root
      open={isOpen}
      onOpenChange={(open) => {
        setSearch('');
        if (open) {
          setPicked(selected);
        } else {
          onApply(picked);
        }
        setIsOpen(open);
      }}
    >
      <Popover.Trigger render={<Button variant="plain" size="xs" />}>
        <Icon name="plus" />
        {label}
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="start" side="bottom" sideOffset={6}>
          <Popover.Popup className="flex max-h-(--available-height) w-90 flex-col overflow-hidden rounded-[14px] p-0 shadow-card">
            <div className="p-2">
              <label className="flex h-9 items-center gap-2 rounded-xl border border-border-ring pr-2.5 pl-3 text-icon-secondary focus-within:border-border-strong">
                <Icon name="search" />
                <input
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder={searchLabel}
                  aria-label={searchLabel}
                  className="min-w-0 flex-1 bg-transparent text-body text-text-primary outline-none placeholder:text-text-tertiary"
                  autoFocus
                />
              </label>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto pb-1.5">
              {groups.map((group) => {
                const items = visible.filter((monitor) => monitor.kind === group.kind);
                if (items.length === 0) {
                  return null;
                }
                return (
                  <div key={group.kind} role="group" aria-label={group.label}>
                    <p className="px-3.5 pt-2.5 pb-1 text-caption font-medium text-text-secondary">{group.label}</p>
                    {items.map((monitor) => {
                      const key = getMonitorKey(monitor);
                      const isHere = picked.includes(key);
                      return (
                        <label
                          key={getMonitorKey(monitor)}
                          className="flex cursor-default items-center gap-2.5 px-3.5 py-1.75 hover:bg-surface-subtle"
                        >
                          <Checkbox checked={isHere} onCheckedChange={() => toggle(key)} />
                          <Icon name={monitor.icon} className="text-icon-secondary" />
                          <span className="min-w-0 flex-1 truncate text-body font-medium text-text-primary">
                            {monitor.name}
                          </span>
                          <span className="shrink-0 text-small text-text-secondary">
                            {describeMonitor?.(key) ?? monitor.caption}
                          </span>
                        </label>
                      );
                    })}
                  </div>
                );
              })}
              {visible.length === 0 && (
                <p className="px-3.5 py-3 text-body text-text-secondary">
                  {monitors.length === 0 ? 'Add an uptime check or cron first.' : 'Nothing matches.'}
                </p>
              )}
            </div>
            <div className="flex items-center gap-2 border-t border-border-ring py-2 pr-2 pl-3.5">
              <p className="min-w-0 flex-1 truncate text-small text-text-secondary">
                {describeCount(picked.length)}
              </p>
              <Popover.Close render={<Button className="w-fit" />}>Done</Popover.Close>
            </div>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
