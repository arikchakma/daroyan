import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { useContext, useId } from 'react';
import type { ReactNode } from 'react';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { MonitorPicker } from '~/components/monitors/monitor-picker';
import { PanelContext } from '~/components/navigation/panel-context';
import { Button } from '~/components/ui/button';
import { Select } from '~/components/ui/select';
import { Sheet } from '~/components/ui/sheet';
import { Switch } from '~/components/ui/switch';
import { listAlertRulesOptions, listIntegrationsOptions } from '~/queries/alerts';
import { listCronsOptions } from '~/queries/crons';
import { listUsersOptions } from '~/queries/incidents';
import { listServersOptions } from '~/queries/servers';
import { listStatusPagesOptions } from '~/queries/status-pages';
import { listUptimeChecksOptions } from '~/queries/uptime';
import { INTEGRATION_KINDS, isIntegrationKind } from '~/utils/alerts';
import { cn } from '~/utils/classname';
import { COMPONENT_STATUS_META, RELATED_LABELS, joinNames } from '~/utils/incidents';
import { getMonitorKey } from '~/utils/status-pages';
import type { Monitor } from '~/utils/status-pages';

export type AffectedComponent = Schemas['IncidentComponentRef'];
export type RelatedItem = Schemas['IncidentRelatedRef'];

const RELATED_GROUPS = [
  { kind: 'rule', label: 'Alerts' },
  { kind: 'server', label: 'Servers' },
  { kind: 'check', label: 'Uptime Checks' },
  { kind: 'cron', label: 'Crons' },
];

export function useIncidentDirectory() {
  const { data: checks = [] } = useQuery(listUptimeChecksOptions());
  const { data: crons = [] } = useQuery(listCronsOptions());
  const { data: servers = [] } = useQuery(listServersOptions());
  const { data: rules = [] } = useQuery(listAlertRulesOptions());
  const { data: users = [] } = useQuery(listUsersOptions());
  const { data: integrations = [] } = useQuery(listIntegrationsOptions());
  const { data: pages = [] } = useQuery(listStatusPagesOptions());

  const monitors: Monitor[] = [
    ...checks.map((check) => ({
      kind: 'check',
      id: check.id,
      name: check.name,
      caption: check.target,
      icon: 'activity' as const,
    })),
    ...crons.map((cron) => ({ kind: 'cron', id: cron.id, name: cron.name, caption: 'Cron', icon: 'clock' as const })),
  ];
  const related: Monitor[] = [
    ...rules.map((rule) => ({ kind: 'rule', id: rule.id, name: rule.name, caption: 'Alert', icon: 'bell' as const })),
    ...servers.map((server) => ({
      kind: 'server',
      id: server.id,
      name: server.name,
      caption: 'Server',
      icon: 'server' as const,
    })),
    ...monitors,
  ];
  const channels = integrations.filter((integration) => integration.kind !== 'webhook');

  return { monitors, related, users, channels, pages };
}

type IncidentDirectory = ReturnType<typeof useIncidentDirectory>;

export function findPagesFor(directory: IncidentDirectory, components: AffectedComponent[]) {
  const keys = new Set(components.map(getMonitorKey));
  return directory.pages
    .map((page) => ({
      page,
      names: page.sections
        .flatMap((section) => section.components)
        .filter((component) => keys.has(getMonitorKey(component)))
        .map((component) => component.name),
    }))
    .filter((entry) => entry.names.length > 0);
}

type IncidentSheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  icon: IconName;
  title: string;
  submitLabel: string;
  isPending: boolean;
  onSubmit: () => void;
  children: ReactNode;
  aside: ReactNode;
};

export function IncidentSheet(props: IncidentSheetProps) {
  const { open, onOpenChange, icon, title, submitLabel, isPending, onSubmit, children, aside } = props;

  const panel = useContext(PanelContext);

  return (
    <Sheet.Root side="bottom" open={open} onOpenChange={onOpenChange}>
      <Sheet.Portal container={panel}>
        <Sheet.Backdrop className="bg-black opacity-[calc(0.3*(1-var(--drawer-swipe-progress)))]" />
        <Sheet.Viewport className={cn('pt-4 pb-1', panel ? 'absolute pr-1' : 'fixed px-1')}>
          <Sheet.Popup className="h-auto flex-1 gap-0 overflow-hidden rounded-2xl border-0 bg-surface-page text-text-primary shadow-popover ring-1 ring-border-elevated transition-[translate,opacity] ring-inset data-ending-style:translate-y-4 data-ending-style:opacity-0 data-starting-style:translate-y-4 data-starting-style:opacity-0">
            <form
              className="flex h-full min-h-0 flex-col"
              onSubmit={(event) => {
                event.preventDefault();
                onSubmit();
              }}
            >
              <div className="flex h-14 shrink-0 items-center gap-3 border-b border-border-row px-4">
                <Icon name={icon} className="shrink-0" />
                <Sheet.Title className="min-w-0 flex-1 truncate text-body font-medium">{title}</Sheet.Title>
                <Button type="submit" disabled={isPending} className="w-fit">
                  {submitLabel}
                </Button>
                <Sheet.Close
                  render={<Button type="button" variant="secondary" size="icon" aria-label="Close" />}
                >
                  <Icon name="x" />
                </Sheet.Close>
              </div>
              <div className="flex min-h-0 flex-1 items-start overflow-y-auto overscroll-contain max-lg:flex-col">
                <div className="flex w-full min-w-0 flex-1 flex-col gap-6 px-10 py-7 max-md:px-4">{children}</div>
                <aside className="flex w-105 shrink-0 flex-col gap-3 pt-6 pr-10 max-lg:w-full max-lg:px-10 max-lg:pb-8 max-md:px-4">
                  {aside}
                </aside>
              </div>
            </form>
          </Sheet.Popup>
        </Sheet.Viewport>
      </Sheet.Portal>
    </Sheet.Root>
  );
}

type SheetRowProps = {
  label: string;
  labelId?: string;
  htmlFor?: string;
  isTall?: boolean;
  children: ReactNode;
};

export function SheetRow(props: SheetRowProps) {
  const { label, labelId, htmlFor, isTall = false, children } = props;

  const LabelTag = htmlFor ? 'label' : 'span';

  return (
    <div
      className={cn(
        'flex gap-4 pr-3 pl-4 max-sm:flex-col max-sm:gap-1.5',
        isTall ? 'items-start py-3' : 'min-h-11 items-center py-2'
      )}
    >
      <LabelTag
        id={labelId}
        htmlFor={htmlFor}
        className={cn('w-30 shrink-0 text-caption font-medium text-text-primary', isTall && 'pt-1.5')}
      >
        {label}
      </LabelTag>
      <div className="flex min-w-0 flex-1 items-center gap-1.5">{children}</div>
    </div>
  );
}

type SegmentedProps<T extends string> = {
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
};

export function Segmented<T extends string>(props: SegmentedProps<T>) {
  const { label, value, options, onChange } = props;

  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap items-center gap-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
          className="h-7 rounded-sm px-2.5 text-body text-text-secondary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 aria-checked:bg-surface-fill aria-checked:font-medium aria-checked:text-text-primary"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

type OptionSelectProps = {
  label: string;
  value: string | null;
  placeholder?: string;
  items: { value: string; label: string }[];
  onChange: (value: string | null) => void;
};

export function OptionSelect(props: OptionSelectProps) {
  const { label, value, placeholder, items, onChange } = props;

  return (
    <Select.Root items={items} value={value} onValueChange={(next) => onChange(next as string | null)}>
      <Select.Trigger aria-label={label} className="w-full">
        <Select.Value placeholder={placeholder} />
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner>
          <Select.Popup>
            {items.map((item) => (
              <Select.Item key={item.value} value={item.value}>
                {item.label}
              </Select.Item>
            ))}
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

type CardProps = {
  children: ReactNode;
  className?: string;
};

export function FormCard(props: CardProps) {
  const { children, className } = props;

  return (
    <div
      className={cn(
        'flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row',
        className
      )}
    >
      {children}
    </div>
  );
}

type SectionProps = {
  title: string;
  description?: string;
  children: ReactNode;
};

export function SheetSection(props: SectionProps) {
  const { title, description, children } = props;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-title-section font-medium">{title}</h2>
        {description && <p className="text-body text-text-secondary">{description}</p>}
      </div>
      {children}
    </section>
  );
}

const COMPONENT_STATUS_ITEMS = (['outage', 'partial', 'degraded'] as const).map((status) => ({
  value: status,
  label: COMPONENT_STATUS_META[status]!.label,
}));

type AffectedComponentsProps = {
  directory: IncidentDirectory;
  components: AffectedComponent[];
  onChange: (components: AffectedComponent[]) => void;
  isMaintenance?: boolean;
};

export function AffectedComponentsCard(props: AffectedComponentsProps) {
  const { directory, components, onChange, isMaintenance = false } = props;

  const names = new Map(directory.monitors.map((monitor) => [getMonitorKey(monitor), monitor.name]));

  return (
    <FormCard>
      {components.map((component, index) => {
        const name = names.get(getMonitorKey(component)) ?? 'Deleted monitor';
        return (
          <SheetRow key={getMonitorKey(component)} label={name}>
            {isMaintenance ? (
              <span className="flex-1 text-body text-tone-info-fg">Under Maintenance</span>
            ) : (
              <OptionSelect
                label={`${name} status`}
                value={component.status ?? null}
                items={COMPONENT_STATUS_ITEMS}
                onChange={(status) =>
                  onChange(
                    components.map((other, otherIndex) =>
                      otherIndex === index ? { ...other, status: status ?? 'outage' } : other
                    )
                  )
                }
              />
            )}
            <Button
              type="button"
              variant="plain"
              size="icon"
              aria-label={`Remove ${name}`}
              onClick={() => onChange(components.filter((_, otherIndex) => otherIndex !== index))}
            >
              <Icon name="x" />
            </Button>
          </SheetRow>
        );
      })}
      <div className="px-2 py-1.5">
        <MonitorPicker
          label="Add Check or Cron"
          monitors={directory.monitors}
          selected={components.map(getMonitorKey)}
          describeCount={(count) => (count === 1 ? '1 affected' : `${count} affected`)}
          onApply={(keys) => {
            const kept = components.filter((component) => keys.includes(getMonitorKey(component)));
            const added = keys
              .filter((key) => !components.some((component) => getMonitorKey(component) === key))
              .map((key) => {
                const [kind = '', id = ''] = key.split(':');
                return { kind, id, status: isMaintenance ? 'maintenance' : 'outage' };
              });
            onChange([...kept, ...added]);
          }}
        />
      </div>
    </FormCard>
  );
}

type RelatedPickerProps = {
  directory: IncidentDirectory;
  related: RelatedItem[];
  onChange: (related: RelatedItem[]) => void;
};

export function RelatedField(props: RelatedPickerProps) {
  const { directory, related, onChange } = props;

  const names = new Map(directory.related.map((item) => [getMonitorKey(item), item]));

  return (
    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
      {related.map((item) => {
        const known = names.get(getMonitorKey(item));
        return (
          <span
            key={getMonitorKey(item)}
            className="flex h-6 items-center gap-1 rounded-xs bg-surface-fill pr-1 pl-1.75 text-caption text-text-secondary"
          >
            {known ? `${RELATED_LABELS[item.kind] ?? item.kind}: ${known.name}` : 'Deleted'}
            <button
              type="button"
              aria-label={`Remove ${known?.name ?? 'item'}`}
              onClick={() => onChange(related.filter((other) => getMonitorKey(other) !== getMonitorKey(item)))}
              className="flex size-4 items-center justify-center rounded-xs text-text-tertiary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 [&_svg]:size-3"
            >
              <Icon name="x" />
            </button>
          </span>
        );
      })}
      <MonitorPicker
        label="Add"
        monitors={directory.related}
        groups={RELATED_GROUPS}
        searchLabel="Search alerts, servers, checks and crons"
        selected={related.map(getMonitorKey)}
        describeCount={(count) => (count === 1 ? '1 related' : `${count} related`)}
        onApply={(keys) =>
          onChange(
            keys.map((key) => {
              const [kind = '', id = ''] = key.split(':');
              return { kind, id };
            })
          )
        }
      />
    </div>
  );
}

type NotifyColumnProps = {
  directory: IncidentDirectory;
  components: AffectedComponent[];
  hiddenPageIds: string[];
  onHiddenPageIdsChange: (ids: string[]) => void;
  channelIds: string[];
  onChannelIdsChange: (ids: string[]) => void;
};

export function NotifyColumn(props: NotifyColumnProps) {
  const { directory, components, hiddenPageIds, onHiddenPageIdsChange, channelIds, onChannelIdsChange } = props;

  const pages = findPagesFor(directory, components);

  return (
    <>
      <h2 className="text-title-section font-medium">Show on Status Pages</h2>
      <FormCard>
        {pages.length === 0 ? (
          <p className="px-4 py-3 text-body text-text-secondary">
            {components.length === 0
              ? 'Add a check or cron to show this on the status pages that include it.'
              : 'No status page includes these components, so this stays internal.'}
          </p>
        ) : (
          pages.map(({ page, names }) => (
            <ToggleRow
              key={page.id}
              title={page.name}
              description={`${joinNames(names)} ${names.length === 1 ? 'is' : 'are'} on it`}
              checked={!hiddenPageIds.includes(page.id)}
              onCheckedChange={(checked) =>
                onHiddenPageIdsChange(
                  checked ? hiddenPageIds.filter((id) => id !== page.id) : [...hiddenPageIds, page.id]
                )
              }
            />
          ))
        )}
      </FormCard>
      <h2 className="mt-3 text-title-section font-medium">Notify Channels</h2>
      <FormCard>
        {directory.channels.length === 0 ? (
          <p className="px-4 py-3 text-body text-text-secondary">
            Connect Slack, Discord or Telegram in Settings to post updates there.
          </p>
        ) : (
          directory.channels.map((channel) => (
            <ToggleRow
              key={channel.id}
              icon={isIntegrationKind(channel.kind) ? INTEGRATION_KINDS[channel.kind].icon : 'plug'}
              title={isIntegrationKind(channel.kind) ? INTEGRATION_KINDS[channel.kind].label : channel.kind}
              description={channel.name}
              checked={channelIds.includes(channel.id)}
              onCheckedChange={(checked) =>
                onChannelIdsChange(
                  checked ? [...channelIds, channel.id] : channelIds.filter((id) => id !== channel.id)
                )
              }
            />
          ))
        )}
      </FormCard>
    </>
  );
}

type ToggleRowProps = {
  icon?: IconName;
  title: string;
  description: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
};

export function ToggleRow(props: ToggleRowProps) {
  const { icon, title, description, checked, onCheckedChange } = props;

  const titleId = useId();

  return (
    <label className="flex cursor-default items-center gap-3 px-4 py-3">
      {icon && <Icon name={icon} className="shrink-0" />}
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={titleId} className="truncate text-body font-medium">
          {title}
        </span>
        <span className="truncate text-small text-text-secondary">{description}</span>
      </span>
      <Switch.Root aria-labelledby={titleId} checked={checked} onCheckedChange={onCheckedChange}>
        <Switch.Thumb />
      </Switch.Root>
    </label>
  );
}
