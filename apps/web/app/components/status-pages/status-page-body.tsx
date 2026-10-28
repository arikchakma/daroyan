import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { Fragment, useId, useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { FormSection } from '~/components/alerts/form-section';
import { Icon } from '~/components/icons/icon';
import { PageStatusBadge } from '~/components/status-pages/status-page-badges';
import { useMonitors } from '~/components/status-pages/status-page-form';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Chip } from '~/components/ui/chip';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { SaveBar } from '~/components/ui/save-bar';
import { Switch } from '~/components/ui/switch';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  deleteStatusPageOptions,
  publicStatusPageOptions,
  toStatusPageInput,
  updateStatusPageOptions,
} from '~/queries/status-pages';
import { formatRelative } from '~/utils/format';
import { getMonitorKey, getPublicUrl } from '~/utils/status-pages';
import { formatUptime } from '~/utils/uptime';

const COMPONENT_BADGES = {
  operational: { tone: 'success', icon: 'check', label: 'Operational' },
  degraded: { tone: 'warning', icon: 'triangle-alert', label: 'Degraded' },
  outage: { tone: 'critical', icon: 'circle-x', label: 'Major Outage' },
  unknown: { tone: 'neutral', icon: 'circle-dashed', label: 'No Data' },
} as const;

type StatusPageBodyProps = {
  page: Schemas['StatusPage'];
};

export function StatusPageBody(props: StatusPageBodyProps) {
  const { page } = props;

  const navigate = useNavigate();
  const publicId = useId();
  const [copy] = useCopyToClipboard();
  const monitors = useMonitors();
  const isAdmin = useIsAdmin();
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const [name, setName] = useState(page.name);
  const [slug, setSlug] = useState(page.slug);
  const [isPublic, setIsPublic] = useState(page.is_public);
  const { data: report } = useQuery(publicStatusPageOptions(page.slug));

  const update = useMutation({
    ...updateStatusPageOptions(),
    meta: { errorMessage: 'The status page could not be saved' },
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: ['status-pages'] });
      queryClient.invalidateQueries({ queryKey: ['status'] });
      toast.success(`Saved ${saved.name}`);
    },
  });
  const remove = useMutation({
    ...deleteStatusPageOptions(),
    meta: { errorMessage: 'The status page could not be deleted' },
    onSuccess: () => {
      navigate('/status-pages');
      queryClient.invalidateQueries({ queryKey: ['status-pages'], exact: true });
      toast.success(`Deleted ${page.name}`);
    },
  });

  const publicUrl = getPublicUrl(page.slug);
  const host = publicUrl.replace(/^https?:\/\//, '');
  const copyUrl = () => void copy(publicUrl).then((isCopied) => isCopied && toast.success('Copied the public URL'));
  const isDirty = name !== page.name || slug !== page.slug || isPublic !== page.is_public;
  const canSave = !!name.trim() && !!slug.trim();
  const save = () =>
    update.mutate({
      pageId: page.id,
      body: { ...toStatusPageInput(page), name: name.trim(), slug: slug.trim(), is_public: isPublic },
    });

  const statuses = new Map(
    (report?.sections ?? []).flatMap((section) => section.components).map((component) => [component.name, component])
  );
  const uptimes = (report?.sections ?? [])
    .flatMap((section) => section.components)
    .map((component) => component.uptime)
    .filter((uptime): uptime is number => typeof uptime === 'number');
  const uptime = uptimes.length > 0 ? uptimes.reduce((sum, value) => sum + value, 0) / uptimes.length : null;
  const active = report?.active_incidents ?? [];
  const past = report?.past_incidents ?? [];

  useCommandActions(page.name, [
    {
      label: 'View Page',
      icon: 'external-link',
      onSelect: () => window.open(`/s/${page.slug}`, '_blank', 'noopener,noreferrer'),
    },
    { label: 'Copy Public URL', icon: 'copy', onSelect: copyUrl },
    isAdmin && {
      label: 'Edit Status Page',
      icon: 'pencil',
      onSelect: () => void navigate(`/status-pages/${page.id}/edit`),
    },
    isAdmin && { label: 'Delete Status Page', icon: 'trash', onSelect: () => setIsDeleteOpen(true) },
  ]);

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo="/status-pages" icon="globe" title="Status Pages">
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            More Actions
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup>
                {isAdmin && (
                  <Menu.LinkItem render={<Link to={`/status-pages/${page.id}/edit`} />}>
                    <Icon name="pencil" />
                    Edit Status Page
                  </Menu.LinkItem>
                )}
                <Menu.Item onClick={copyUrl}>
                  <Icon name="copy" />
                  Copy Public URL
                </Menu.Item>
                {isAdmin && (
                  <>
                    <Menu.Separator />
                    <Menu.Item onClick={() => setIsDeleteOpen(true)} className="text-tone-critical-fg">
                      <Icon name="trash" />
                      Delete Status Page
                    </Menu.Item>
                  </>
                )}
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
        <Button
          nativeButton={false}
          className="w-fit"
          render={<a href={`/s/${page.slug}`} target="_blank" rel="noreferrer" />}
        >
          <Icon name="external-link" />
          View Page
        </Button>
      </PageBar>
      <SaveBar
        isDirty={isDirty}
        isSaving={update.isPending}
        canSave={canSave}
        onSave={save}
        onDiscard={() => {
          setName(page.name);
          setSlug(page.slug);
          setIsPublic(page.is_public);
        }}
      />

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:px-4">
        <div className="flex w-200 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <div className="flex items-center gap-2.5">
              <h1 className="truncate text-title-page font-semibold">{page.name}</h1>
              <PageStatusBadge status={page.status} />
            </div>
            <p className="truncate text-body text-text-secondary">
              {host} · {page.is_public ? 'Public' : 'Private'} · Updated {formatRelative(page.updated_at)}
            </p>
          </div>

          <StatStrip columns={4}>
            <Stat label="Components" value={page.component_count.toString()} />
            <Stat label="Sections" value={page.sections.length.toString()} />
            <Stat label="Uptime" value={formatUptime(uptime)} detail="90 days" />
            <Stat label="Incidents" value={report ? past.length.toString() : '—'} detail="14 days" />
          </StatStrip>

          <FormSection
            title="Components"
            description="Each component takes its status from the uptime check or cron linked to it."
            action={
              isAdmin && (
                <Button
                  nativeButton={false}
                  variant="secondary"
                  className="w-fit"
                  render={<Link to={`/status-pages/${page.id}/edit`} />}
                >
                  <Icon name="plus" />
                  Add Component
                </Button>
              )
            }
          >
            <div role="table" aria-label="Components" className="flex flex-col">
              <div
                role="row"
                className="flex h-8 items-center gap-3 bg-surface-row px-4 text-caption font-medium text-text-secondary"
              >
                <span role="columnheader" className="min-w-0 flex-1">
                  Component
                </span>
                <span role="columnheader" className="w-65 max-md:hidden">
                  Linked To
                </span>
                <span role="columnheader" className="w-35">
                  Status
                </span>
              </div>
              {page.component_count === 0 && (
                <p className="px-4 py-3 text-body text-text-secondary">No components yet. Add one to fill the page.</p>
              )}
              {page.sections.map((section, sectionIndex) => (
                <Fragment key={sectionIndex}>
                  {page.sections.length > 1 && (
                    <div className="border-t border-border-row px-4 pt-3 pb-1 text-caption font-medium text-text-tertiary">
                      {section.name}
                    </div>
                  )}
                  {section.components.map((component) => {
                    const monitor = monitors.get(getMonitorKey(component));
                    const status = statuses.get(component.name)?.status ?? 'unknown';
                    const badge = COMPONENT_BADGES[status as keyof typeof COMPONENT_BADGES] ?? COMPONENT_BADGES.unknown;
                    return (
                      <div
                        key={getMonitorKey(component)}
                        role="row"
                        className="flex h-11 items-center gap-3 border-t border-border-row px-4 first-of-type:border-t-0"
                      >
                        <span role="cell" className="min-w-0 flex-1 truncate text-body font-medium">
                          {component.name}
                        </span>
                        <span role="cell" className="flex w-65 min-w-0 gap-1.5 max-md:hidden">
                          <Chip>{monitor?.name ?? 'Deleted monitor'}</Chip>
                        </span>
                        <span role="cell" className="w-35">
                          <Badge tone={badge.tone}>
                            <Icon name={badge.icon} />
                            {badge.label}
                          </Badge>
                        </span>
                      </div>
                    );
                  })}
                </Fragment>
              ))}
            </div>
          </FormSection>

          <FormSection title="Page">
            <div className="flex flex-col gap-5 p-4">
              <FieldRow label="Name">
                {({ inputId }) => (
                  <Input id={inputId} value={name} onValueChange={setName} maxLength={100} required disabled={!isAdmin} />
                )}
              </FieldRow>
            </div>
            <div className="p-4">
              <FieldRow
                label="Slug"
                hint={`Your page lives at ${window.location.host}/s/${slug || 'your-slug'}. Changing it breaks old links.`}
              >
                {({ inputId, hintId }) => (
                  <Input
                    id={inputId}
                    aria-describedby={hintId}
                    value={slug}
                    onValueChange={(value) => setSlug(value.toLowerCase().replace(/[^a-z0-9-]/g, '-').slice(0, 48))}
                    required
                    disabled={!isAdmin}
                  />
                )}
              </FieldRow>
            </div>
            <div className="flex items-center gap-3 px-4 py-3.5">
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span id={publicId} className="text-body font-medium">
                  Public Page
                </span>
                <span className="text-body text-text-secondary">
                  Anyone with the link can see it. Turn it off to keep it to your team.
                </span>
              </div>
              <Switch.Root
                aria-labelledby={publicId}
                checked={isPublic}
                onCheckedChange={setIsPublic}
                disabled={!isAdmin}
              >
                <Switch.Thumb />
              </Switch.Root>
            </div>
          </FormSection>
        </div>

        <aside aria-label="Details" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:hidden">
          <RailSection title="Link">
            <RailRow label="Host">
              <span className="font-mono text-code-small">{host}</span>
            </RailRow>
            <RailRow label="Visibility">{page.is_public ? 'Public' : 'Private'}</RailRow>
            <RailRow label="Refresh">Every 30 seconds</RailRow>
            <Button variant="secondary" className="w-fit" onClick={copyUrl}>
              <Icon name="copy" />
              Copy URL
            </Button>
          </RailSection>
          <RailSection title={active.length > 1 ? 'Active Incidents' : 'Active Incident'} isLast>
            {active.length === 0 ? (
              <p className="text-body text-text-secondary">None right now.</p>
            ) : (
              active.map((incident) => (
                <RailRow
                  key={incident.id}
                  label={incident.title}
                >
                  <Link
                    to={`/incidents/${incident.id}`}
                    className="outline-none hover:underline focus-visible:underline"
                  >
                    {formatRelative(incident.started_at)}
                  </Link>
                </RailRow>
              ))
            )}
          </RailSection>
        </aside>
      </div>

      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${page.name}?`}
        description="Its public link stops working. The checks and crons on it stay."
        confirmLabel="Delete Status Page"
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ pageId: page.id })}
      />
    </div>
  );
}
