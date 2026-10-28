import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useId, useRef, useState } from 'react';
import type { DragEvent, KeyboardEvent, ReactNode } from 'react';
import { Link, useNavigate } from 'react-router';

import { FormSection } from '~/components/alerts/form-section';
import { Icon } from '~/components/icons/icon';
import { MonitorPicker } from '~/components/monitors/monitor-picker';
import { Button } from '~/components/ui/button';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { Switch } from '~/components/ui/switch';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listCronsOptions } from '~/queries/crons';
import { createStatusPageOptions, updateStatusPageOptions } from '~/queries/status-pages';
import type { StatusPageInput } from '~/queries/status-pages';
import { listUptimeChecksOptions } from '~/queries/uptime';
import { cn } from '~/utils/classname';
import { describeSchedule } from '~/utils/crons';
import { getMonitorKey } from '~/utils/status-pages';
import type { Monitor } from '~/utils/status-pages';
import { formatInterval } from '~/utils/uptime';

const MAX_LOGO_BYTES = 200 * 1024;
const LOGO_TYPES = 'image/png,image/jpeg,image/webp,image/svg+xml';

type Section = { key: string; name: string; components: Schemas['StatusPageComponent'][] };

function toSlug(value: string) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
}

const SECTION_DRAG_TYPE = 'application/status-page-section';
const COMPONENT_DRAG_TYPE = 'application/status-page-component';

type DropPosition = 'none' | 'top' | 'bottom';

function readTransfer<T>(event: DragEvent, type: string): T | null {
  const raw = event.dataTransfer.getData(type);
  if (!raw) {
    return null;
  }
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function reorder<T>(list: T[], from: number, targetIndex: number) {
  if (from === targetIndex || from === targetIndex - 1) {
    return list;
  }
  return move(list, from, from < targetIndex ? targetIndex - 1 : targetIndex);
}

function move<T>(list: T[], from: number, to: number) {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export function useMonitors() {
  const { data: checks = [] } = useQuery(listUptimeChecksOptions());
  const { data: crons = [] } = useQuery(listCronsOptions());

  const monitors: Monitor[] = [
    ...checks.map((check) => ({
      kind: 'check',
      id: check.id,
      name: check.name,
      caption: `${check.kind === 'tcp' ? 'TCP' : 'Uptime check'} · every ${formatInterval(check.interval_secs)}`,
      icon: 'activity' as const,
    })),
    ...crons.map((cron) => ({
      kind: 'cron',
      id: cron.id,
      name: cron.name,
      caption: `Cron · ${describeSchedule(cron)}`,
      icon: 'clock' as const,
    })),
  ];
  return new Map(monitors.map((monitor) => [getMonitorKey(monitor), monitor]));
}

type StatusPageFormProps = {
  page?: Schemas['StatusPage'];
};

export function StatusPageForm(props: StatusPageFormProps) {
  const { page } = props;

  const [version, setVersion] = useState(0);

  return <StatusPageFormBody key={version} page={page} onDiscard={() => setVersion(version + 1)} />;
}

type StatusPageFormBodyProps = StatusPageFormProps & {
  onDiscard: () => void;
};

function StatusPageFormBody(props: StatusPageFormBodyProps) {
  const { page, onDiscard } = props;

  const formRef = useRef<HTMLFormElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const keyRef = useRef(0);
  const navigate = useNavigate();
  const publicId = useId();
  const monitors = useMonitors();

  const nextKey = () => {
    keyRef.current += 1;
    return `section-${keyRef.current}`;
  };

  const [name, setName] = useState(page?.name ?? '');
  const [slug, setSlug] = useState(page?.slug ?? '');
  const [isSlugEdited, setIsSlugEdited] = useState(!!page);
  const [logoUrl, setLogoUrl] = useState(page?.logo_url ?? null);
  const [isPublic, setIsPublic] = useState(page?.is_public ?? true);
  const [sections, setSections] = useState<Section[]>(() =>
    page && page.sections.length > 0
      ? page.sections.map((section) => ({ ...section, key: nextKey() }))
      : [{ key: nextKey(), name: 'Services', components: [] }]
  );

  const body: StatusPageInput = {
    name: name.trim(),
    slug: slug.trim(),
    logo_url: logoUrl,
    is_public: isPublic,
    sections: sections.map((section) => ({
      name: section.name.trim(),
      components: section.components.map((component) => ({ ...component, name: component.name.trim() })),
    })),
  };

  const onSaved = (saved: Schemas['StatusPage']) => {
    queryClient.invalidateQueries({ queryKey: ['status-pages'] });
    queryClient.invalidateQueries({ queryKey: ['status'] });
    toast.success(page ? `Saved ${saved.name}` : `Created ${saved.name}`);
    navigate(`/status-pages/${saved.id}`);
  };
  const create = useMutation({
    ...createStatusPageOptions(),
    onSuccess: onSaved,
    meta: { errorMessage: 'The status page could not be created' },
  });
  const update = useMutation({
    ...updateStatusPageOptions(),
    onSuccess: onSaved,
    meta: { errorMessage: 'The status page could not be saved' },
  });

  const componentCount = sections.reduce((count, section) => count + section.components.length, 0);
  const canSave =
    !!body.name &&
    !!body.slug &&
    sections.every(
      (section) => !!section.name.trim() && section.components.every((component) => !!component.name.trim())
    );
  const snapshot = JSON.stringify(body);
  const [savedSnapshot] = useState(snapshot);
  const isSaving = create.isPending || update.isPending;
  const backTo = page ? `/status-pages/${page.id}` : '/status-pages';
  const host = window.location.host;

  const save = () => {
    if (page) {
      update.mutate({ pageId: page.id, body });
    } else {
      create.mutate(body);
    }
  };

  const updateSection = (index: number, patch: Partial<Section>) =>
    setSections((current) => current.map((section, at) => (at === index ? { ...section, ...patch } : section)));

  const applyMonitors = (sectionIndex: number, keys: string[]) =>
    setSections((current) => {
      const all = current.flatMap((section) => section.components);
      return current.map((section, at) => {
        if (at !== sectionIndex) {
          return {
            ...section,
            components: section.components.filter((component) => !keys.includes(getMonitorKey(component))),
          };
        }
        const kept = section.components.filter((component) => keys.includes(getMonitorKey(component)));
        const added = keys
          .filter((key) => !kept.some((component) => getMonitorKey(component) === key))
          .map((key) => {
            const existing = all.find((component) => getMonitorKey(component) === key);
            const monitor = monitors.get(key);
            return existing ?? { kind: monitor?.kind ?? '', id: monitor?.id ?? '', name: monitor?.name ?? '' };
          });
        return { ...section, components: [...kept, ...added] };
      });
    });

  const onLogoChange = (file: File | undefined) => {
    if (!file) {
      return;
    }
    if (file.size > MAX_LOGO_BYTES) {
      toast.error('The logo can be at most 200 KB');
      return;
    }
    const reader = new FileReader();
    reader.onload = () => setLogoUrl(typeof reader.result === 'string' ? reader.result : null);
    reader.readAsDataURL(file);
  };

  const dropSection = (event: DragEvent, targetIndex: number) => {
    const transfer = readTransfer<{ sectionKey: string }>(event, SECTION_DRAG_TYPE);
    const from = sections.findIndex((section) => section.key === transfer?.sectionKey);
    if (from !== -1) {
      setSections((current) => reorder(current, from, targetIndex));
    }
  };

  const dropComponent = (event: DragEvent, sectionIndex: number, targetIndex: number) => {
    const transfer = readTransfer<{ sectionKey: string; key: string }>(event, COMPONENT_DRAG_TYPE);
    const fromSection = sections.findIndex((section) => section.key === transfer?.sectionKey);
    if (!transfer || fromSection === -1) {
      return;
    }
    const from = sections[fromSection].components.findIndex((component) => getMonitorKey(component) === transfer.key);
    if (from === -1) {
      return;
    }
    if (fromSection === sectionIndex) {
      updateSection(sectionIndex, { components: reorder(sections[sectionIndex].components, from, targetIndex) });
      return;
    }
    const dragged = sections[fromSection].components[from];
    setSections((current) =>
      current.map((section, at) => {
        if (at === fromSection) {
          return { ...section, components: section.components.filter((_, index) => index !== from) };
        }
        if (at === sectionIndex) {
          const components = [...section.components];
          components.splice(targetIndex, 0, dragged);
          return { ...section, components };
        }
        return section;
      })
    );
  };

  const onHandleKeyDown = (event: KeyboardEvent, section: number, component: number | null) => {
    const step = event.key === 'ArrowUp' ? -1 : event.key === 'ArrowDown' ? 1 : 0;
    if (!step) {
      return;
    }
    event.preventDefault();
    if (component === null) {
      const to = section + step;
      if (to >= 0 && to < sections.length) {
        setSections((current) => move(current, section, to));
      }
      return;
    }
    const to = component + step;
    if (to >= 0 && to < sections[section].components.length) {
      updateSection(section, { components: move(sections[section].components, component, to) });
    }
  };

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo={backTo} icon="globe" title="Status Pages">
        {!page && (
          <>
            <Button nativeButton={false} variant="secondary" className="w-fit" render={<Link to={backTo} />}>
              Cancel
            </Button>
            <Button className="w-fit" disabled={!canSave || isSaving} onClick={() => formRef.current?.requestSubmit()}>
              {isSaving && <Icon name="loader" className="animate-spin" />}
              Create Status Page
            </Button>
          </>
        )}
      </PageBar>
      {page && (
        <SaveBar
          isDirty={snapshot !== savedSnapshot}
          isSaving={isSaving}
          canSave={canSave}
          onSave={() => formRef.current?.requestSubmit()}
          onDiscard={onDiscard}
        />
      )}

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:px-4">
        <form
          ref={formRef}
          onSubmit={(event) => {
            event.preventDefault();
            if (canSave) {
              save();
            }
          }}
          className="flex w-200 max-w-full min-w-0 flex-col gap-8"
        >
          <div className="flex flex-col gap-1">
            <h1 className="text-title-page font-semibold">{page ? page.name : 'New Status Page'}</h1>
            <p className="text-body text-text-secondary">
              A public page that shows the health of the uptime checks and crons you pick. Visitors don&apos;t need to
              sign in.
            </p>
          </div>

          <FormSection title="Page">
            <div className="flex flex-col gap-5 p-4">
              <FieldRow label="Name" hint="Shown at the top of the page and in the browser tab.">
                {({ inputId, hintId }) => (
                  <Input
                    id={inputId}
                    aria-describedby={hintId}
                    value={name}
                    onValueChange={(value) => {
                      setName(value);
                      if (!isSlugEdited) {
                        setSlug(toSlug(value));
                      }
                    }}
                    placeholder="Acme Status"
                    maxLength={100}
                    required
                    autoFocus={!page}
                  />
                )}
              </FieldRow>
              <FieldRow
                label="Slug"
                hint={`Your page lives at ${host}/s/${slug || 'your-slug'}.${page ? ' Changing it breaks old links.' : ''}`}
              >
                {({ inputId, hintId }) => (
                  <Input
                    id={inputId}
                    aria-describedby={hintId}
                    value={slug}
                    onValueChange={(value) => {
                      setIsSlugEdited(true);
                      setSlug(value.toLowerCase().replace(/[^a-z0-9-]/g, '-').slice(0, 48));
                    }}
                    placeholder="acme"
                    required
                  />
                )}
              </FieldRow>
              <div className="flex flex-col gap-1.5">
                <span className="text-body font-medium">Logo</span>
                <div className="flex items-center gap-2">
                  <span className="flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-surface-fill text-icon-secondary">
                    {logoUrl ? <img src={logoUrl} alt="" className="size-full object-cover" /> : <Icon name="image" />}
                  </span>
                  <Button variant="secondary" className="w-fit" onClick={() => fileRef.current?.click()}>
                    <Icon name="upload" />
                    {logoUrl ? 'Replace Image' : 'Upload Image'}
                  </Button>
                  {logoUrl && (
                    <Button variant="plain" className="w-fit" onClick={() => setLogoUrl(null)}>
                      Remove
                    </Button>
                  )}
                  <input
                    ref={fileRef}
                    type="file"
                    accept={LOGO_TYPES}
                    className="hidden"
                    onChange={(event) => {
                      onLogoChange(event.target.files?.[0]);
                      event.target.value = '';
                    }}
                  />
                </div>
                <p className="text-caption text-text-secondary">
                  Optional. PNG, JPEG, WebP or SVG up to 200 KB, shown next to the name. Square works best.
                </p>
              </div>
            </div>
            <div className="flex items-center gap-3 px-4 py-3">
              <Icon name="globe" />
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span id={publicId} className="text-body font-medium">
                  Public
                </span>
                <span className="text-body text-text-secondary">
                  Anyone with the link can see it. Turn it off to keep the page to your team while you set it up.
                </span>
              </div>
              <Switch.Root aria-labelledby={publicId} checked={isPublic} onCheckedChange={setIsPublic}>
                <Switch.Thumb />
              </Switch.Root>
            </div>
          </FormSection>

          <section className="flex flex-col gap-3">
            <div className="flex flex-col gap-0.5">
              <h2 className="text-title-section font-medium">Components</h2>
              <p className="text-body text-text-secondary">
                Group what visitors see into sections. They only see the public name, never the internal one.
              </p>
            </div>
            {sections.map((section, sectionIndex) => (
              <DropTarget
                key={section.key}
                type={SECTION_DRAG_TYPE}
                onDrop={(event, position) => dropSection(event, position === 'top' ? sectionIndex : sectionIndex + 1)}
                className="-my-1.5 py-1.5"
              >
                <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
                  <div className="flex items-center gap-2 px-3 py-2">
                    <DragHandle
                      label={`Move ${section.name || 'section'}`}
                      type={SECTION_DRAG_TYPE}
                      data={{ sectionKey: section.key }}
                      onKeyDown={(event) => onHandleKeyDown(event, sectionIndex, null)}
                    />
                    <Input
                      aria-label="Section name"
                      value={section.name}
                      onValueChange={(value) => updateSection(sectionIndex, { name: value })}
                      placeholder="Section name"
                      maxLength={100}
                      required
                      className="flex-1"
                    />
                    <Button
                      variant="plain"
                      size="icon"
                      aria-label={`Delete ${section.name || 'section'}`}
                      onClick={() => setSections((current) => current.filter((_, at) => at !== sectionIndex))}
                    >
                      <Icon name="trash" />
                    </Button>
                  </div>
                  {section.components.map((component, componentIndex) => {
                    const monitor = monitors.get(getMonitorKey(component));
                    return (
                      <DropTarget
                        key={getMonitorKey(component)}
                        type={COMPONENT_DRAG_TYPE}
                        onDrop={(event, position) =>
                          dropComponent(event, sectionIndex, position === 'top' ? componentIndex : componentIndex + 1)
                        }
                        className="flex items-center gap-2 px-3 py-2"
                      >
                        <DragHandle
                          label={`Move ${component.name || 'component'}`}
                          type={COMPONENT_DRAG_TYPE}
                          data={{ sectionKey: section.key, key: getMonitorKey(component) }}
                          onKeyDown={(event) => onHandleKeyDown(event, sectionIndex, componentIndex)}
                        />
                        <Icon name={monitor?.icon ?? 'circle-help'} className="text-icon-secondary" />
                        <div className="flex min-w-0 flex-1 flex-col">
                          <span className="truncate text-body font-medium">{monitor?.name ?? 'Deleted monitor'}</span>
                          <span className="truncate text-small text-text-secondary">
                            {monitor?.caption ?? 'Remove it to save the page'}
                          </span>
                        </div>
                        <Input
                          aria-label={`Public name for ${monitor?.name ?? 'component'}`}
                          value={component.name}
                          onValueChange={(value) =>
                            updateSection(sectionIndex, {
                              components: section.components.map((other, at) =>
                                at === componentIndex ? { ...other, name: value } : other
                              ),
                            })
                          }
                          placeholder="Public name"
                          maxLength={100}
                          required
                          className="w-72 max-sm:w-36"
                        />
                        <Button
                          variant="plain"
                          size="icon"
                          aria-label={`Remove ${component.name || 'component'}`}
                          onClick={() =>
                            updateSection(sectionIndex, {
                              components: section.components.filter((_, at) => at !== componentIndex),
                            })
                          }
                        >
                          <Icon name="x" />
                        </Button>
                      </DropTarget>
                    );
                  })}
                  <DropTarget
                    type={COMPONENT_DRAG_TYPE}
                    onDrop={(event) => dropComponent(event, sectionIndex, section.components.length)}
                    isEdgeOnly
                    className="bg-surface-subtle px-2 py-1.5"
                  >
                    <MonitorPicker
                      label="Add Component"
                      monitors={[...monitors.values()]}
                      selected={section.components.map(getMonitorKey)}
                      describeCount={(count) => `${count} in ${section.name || 'this section'}`}
                      describeMonitor={(key) => {
                        const index = sections.findIndex((other) =>
                          other.components.some((component) => getMonitorKey(component) === key)
                        );
                        return index >= 0 && index !== sectionIndex
                          ? `In ${sections[index]?.name || 'another section'}`
                          : undefined;
                      }}
                      onApply={(keys) => applyMonitors(sectionIndex, keys)}
                    />
                  </DropTarget>
                </div>
              </DropTarget>
            ))}
            <Button
              variant="secondary"
              className="w-fit"
              onClick={() => setSections((current) => [...current, { key: nextKey(), name: '', components: [] }])}
            >
              <Icon name="plus" />
              Add Section
            </Button>
          </section>
        </form>

        <aside aria-label="Summary" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <div className="flex flex-col gap-2.5 border-b border-border-row pb-4">
            <div className="flex items-center gap-3">
              <h2 className="flex-1 text-body font-medium">Public URL</h2>
              {!page && <span className="text-body text-text-tertiary">Live once created</span>}
            </div>
            <div className="flex flex-col gap-1.5 rounded-xl bg-surface-subtle p-4 text-body text-text-secondary">
              <span className="font-medium break-all text-text-primary">
                {host}/s/{slug || 'your-slug'}
              </span>
              <span>
                {componentCount} {componentCount === 1 ? 'component' : 'components'} in {sections.length}{' '}
                {sections.length === 1 ? 'section' : 'sections'}
              </span>
              <span>{isPublic ? 'Public, no sign-in needed' : 'Private, only your team can see it'}</span>
              <span>Status updates every minute</span>
            </div>
          </div>
          <div className="flex flex-col gap-2.5">
            <h2 className="text-body font-medium">Overall Status</h2>
            <ul className="flex flex-col gap-2 text-body text-text-secondary">
              <StatusRule className="bg-tone-success-solid">All Systems Operational when every component is up</StatusRule>
              <StatusRule className="bg-toast-warning-icon">
                Partial Outage when some components are down or degraded
              </StatusRule>
              <StatusRule className="bg-delta-bad">Major Outage when every component in a section is down</StatusRule>
            </ul>
          </div>
        </aside>
      </div>
    </div>
  );
}

type DropTargetProps = {
  type: string;
  onDrop: (event: DragEvent, position: Exclude<DropPosition, 'none'>) => void;
  isEdgeOnly?: boolean;
  className?: string;
  children: ReactNode;
};

function DropTarget(props: DropTargetProps) {
  const { type, onDrop, isEdgeOnly, className, children } = props;

  const [acceptDrop, setAcceptDrop] = useState<DropPosition>('none');

  return (
    <div
      onDragOver={(event) => {
        if (!event.dataTransfer.types.includes(type)) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        const rect = event.currentTarget.getBoundingClientRect();
        const midpoint = (rect.top + rect.bottom) / 2;
        setAcceptDrop(isEdgeOnly || event.clientY <= midpoint ? 'top' : 'bottom');
      }}
      onDragLeave={() => setAcceptDrop('none')}
      onDrop={(event) => {
        if (acceptDrop === 'none') {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        setAcceptDrop('none');
        onDrop(event, acceptDrop);
      }}
      className={cn('relative', className)}
    >
      {children}
      {acceptDrop !== 'none' && (
        <span
          aria-hidden
          className={cn(
            'pointer-events-none absolute inset-x-0 z-10 h-0.5 rounded-full bg-text-link',
            acceptDrop === 'top' ? '-top-px' : '-bottom-px'
          )}
        />
      )}
    </div>
  );
}

type DragHandleProps = {
  label: string;
  type: string;
  data: Record<string, string>;
  onKeyDown: (event: KeyboardEvent) => void;
};

function DragHandle(props: DragHandleProps) {
  const { label, type, data, onKeyDown } = props;

  return (
    <button
      type="button"
      draggable
      aria-label={label}
      title="Drag, or use the arrow keys, to reorder"
      onDragStart={(event) => {
        event.dataTransfer.effectAllowed = 'move';
        event.dataTransfer.setData(type, JSON.stringify(data));
        const row = event.currentTarget.parentElement;
        if (row) {
          event.dataTransfer.setDragImage(row, 16, 16);
        }
      }}
      onKeyDown={onKeyDown}
      className="flex size-6 shrink-0 cursor-grab items-center justify-center rounded-sm text-icon-secondary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 active:cursor-grabbing"
    >
      <Icon name="grip" />
    </button>
  );
}

type StatusRuleProps = {
  className: string;
  children: string;
};

function StatusRule(props: StatusRuleProps) {
  const { className, children } = props;

  return (
    <li className="flex gap-2">
      <span className={cn('mt-1.75 size-1.5 shrink-0 rounded-full', className)} />
      {children}
    </li>
  );
}
