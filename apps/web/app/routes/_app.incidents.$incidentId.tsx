import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router';

import { IncidentStatusBadge } from '~/components/incidents/incident-badges';
import { IncidentComposer } from '~/components/incidents/incident-composer';
import {
  AddResponderDialog,
  EditComponentsDialog,
  EditIncidentDialog,
  EditUpdateDialog,
  ResolveDialog,
} from '~/components/incidents/incident-dialogs';
import { useIncidentDirectory } from '~/components/incidents/incident-form';
import { IncidentTimeline } from '~/components/incidents/incident-timeline';
import { PostmortemCard } from '~/components/incidents/postmortem-card';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { RailRow, RailSection, Stat, StatStrip } from '~/components/ui/detail-rail';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { Skeleton } from '~/components/ui/skeleton';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import {
  deleteIncidentOptions,
  incidentOptions,
  incidentUpdatesOptions,
  postIncidentUpdateOptions,
} from '~/queries/incidents';
import { formatClock } from '~/utils/format';
import {
  RELATED_LABELS,
  describeShownPages,
  formatCountdown,
  formatIncidentDuration,
  formatMoment,
  formatSpan,
  formatWindow,
  getComponentStatusMeta,
  getRelatedPath,
  getSeverityMeta,
  isClosed,
  isMaintenance,
  joinNames,
} from '~/utils/incidents';
import type { Incident, IncidentUpdate } from '~/utils/incidents';
import { INTEGRATION_KINDS, isIntegrationKind } from '~/utils/alerts';

export function meta() {
  return [{ title: 'Incident · Daroyan' }];
}

function paragraph(text: string) {
  return { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] };
}

export default function IncidentPage() {
  const { incidentId = '' } = useParams();
  const { data: incident, isLoading, isError } = useQuery(incidentOptions(incidentId));

  if (isLoading) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar backTo="/incidents" icon="flag" title="Incidents" />
        <div className="flex justify-center px-10 pt-3 max-lg:px-4">
          <div className="flex w-200 max-w-full flex-col gap-3">
            <Skeleton className="h-8 w-80" />
            <Skeleton className="h-4 w-60" />
            <Skeleton className="mt-4 h-18 w-full" />
          </div>
        </div>
      </div>
    );
  }

  if (isError || !incident) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar backTo="/incidents" icon="flag" title="Incidents" />
        <div className="flex flex-1 flex-col items-center justify-center gap-1 pb-20 text-center">
          <p className="text-title-section font-medium">Incident not found</p>
          <p className="text-body text-text-secondary">It may have been deleted.</p>
          <Button nativeButton={false} variant="secondary" className="mt-3 w-fit" render={<Link to="/incidents" />}>
            Back to Incidents
          </Button>
        </div>
      </div>
    );
  }

  return <IncidentDetail incident={incident} />;
}

function describeSubtitle(incident: Incident) {
  const shown = describeShownPages(incident);
  const pages = shown === 'internal only' ? 'Internal only' : `On ${shown.slice(3)}`;
  if (isMaintenance(incident)) {
    const window = incident.starts_at && incident.ends_at ? formatWindow(incident.starts_at, incident.ends_at) : '';
    return ['Maintenance', window, pages].filter(Boolean).join(' · ');
  }
  const severity = getSeverityMeta(incident.severity)?.label;
  if (incident.resolved_at) {
    return [
      severity,
      `Lasted ${formatIncidentDuration(incident)}`,
      `Resolved ${formatMoment(incident.resolved_at).toLowerCase()}`,
      pages,
    ]
      .filter(Boolean)
      .join(' · ');
  }
  return [severity, `Started ${formatMoment(incident.started_at).toLowerCase()}`, pages].filter(Boolean).join(' · ');
}

function IncidentDetail(props: { incident: Incident }) {
  const { incident } = props;

  const navigate = useNavigate();
  const [copy] = useCopyToClipboard();
  const directory = useIncidentDirectory();
  const { data: updates = [] } = useQuery(incidentUpdatesOptions(incident.id));
  const [isResolveOpen, setIsResolveOpen] = useState(false);
  const isAdmin = useIsAdmin();
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const [isEditOpen, setIsEditOpen] = useState(false);
  const [isComponentsOpen, setIsComponentsOpen] = useState(false);
  const [isResponderOpen, setIsResponderOpen] = useState(false);
  const [editing, setEditing] = useState<IncidentUpdate>();

  const maintenance = isMaintenance(incident);
  const closed = isClosed(incident.status);
  const scheduled = incident.status === 'scheduled';
  const shownOn = describeShownPages(incident);
  const shownPages = incident.status_pages.filter((page) => page.is_shown);
  const channels = directory.channels.filter((channel) => incident.channel_ids.includes(channel.id));
  const channelNames = joinNames([
    ...new Set(
      channels.map((channel) => (isIntegrationKind(channel.kind) ? INTEGRATION_KINDS[channel.kind].label : channel.kind))
    ),
  ]);
  const severity = getSeverityMeta(incident.severity);

  const remove = useMutation({
    ...deleteIncidentOptions(),
    meta: { errorMessage: 'The incident could not be deleted' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      toast.success(maintenance ? 'Maintenance deleted' : 'Incident deleted');
      navigate('/incidents');
    },
  });

  const post = useMutation({
    ...postIncidentUpdateOptions(),
    meta: { errorMessage: 'The status could not be changed' },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['incidents'] }),
  });

  const startNow = () =>
    post.mutate(
      {
        incidentId: incident.id,
        body: { kind: 'update', status: 'in_progress', body: paragraph('Maintenance started.'), is_public: true, notify: true },
      },
      { onSuccess: () => toast.success('Maintenance started') }
    );

  const reopen = () =>
    post.mutate(
      {
        incidentId: incident.id,
        body: {
          kind: 'update',
          status: maintenance ? 'in_progress' : 'investigating',
          body: paragraph(maintenance ? 'Maintenance reopened.' : 'Incident reopened.'),
          is_public: false,
          notify: false,
        },
      },
      { onSuccess: () => toast.success(maintenance ? 'Maintenance reopened' : 'Incident reopened') }
    );

  const link = `${window.location.origin}/incidents/${incident.id}`;

  useCommandActions(incident.title, [
    !closed && scheduled && { label: 'Start Now', icon: 'play', onSelect: startNow },
    !closed &&
      !scheduled && {
        label: maintenance ? 'Complete Maintenance' : 'Resolve Incident',
        icon: 'check',
        onSelect: () => setIsResolveOpen(true),
      },
    closed && {
      label: maintenance ? 'Reopen Maintenance' : 'Reopen Incident',
      icon: 'play',
      onSelect: reopen,
    },
    {
      label: maintenance ? 'Edit Maintenance' : 'Edit Incident',
      icon: 'pencil',
      onSelect: () => setIsEditOpen(true),
    },
    {
      label: 'Copy Link',
      icon: 'copy',
      onSelect: () => void copy(link).then((isCopied) => isCopied && toast.success('Copied the link')),
    },
    ...shownPages.map((page) => ({
      label: `View on ${page.name}`,
      icon: 'external-link' as const,
      onSelect: () =>
        window.open(`/s/${page.slug}/incidents/${incident.id}`, '_blank', 'noopener,noreferrer'),
    })),
    isAdmin && {
      label: maintenance ? 'Delete Maintenance' : 'Delete Incident',
      icon: 'trash',
      onSelect: () => setIsDeleteOpen(true),
    },
  ]);

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo="/incidents" icon="flag" title="Incidents">
        <Menu.Root>
          <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
            More Actions
            <Icon name="chevron-down" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup>
                {closed && (
                  <Menu.Item onClick={reopen}>
                    <Icon name="play" />
                    {maintenance ? 'Reopen Maintenance' : 'Reopen Incident'}
                  </Menu.Item>
                )}
                <Menu.Item onClick={() => setIsEditOpen(true)}>
                  <Icon name="pencil" />
                  {maintenance ? 'Edit Maintenance' : 'Edit Incident'}
                </Menu.Item>
                <Menu.Item
                  onClick={() => void copy(link).then((isCopied) => isCopied && toast.success('Copied the link'))}
                >
                  <Icon name="copy" />
                  Copy Link
                </Menu.Item>
                {shownPages.map((page) => (
                  <Menu.LinkItem
                    key={page.id}
                    render={<a href={`/s/${page.slug}/incidents/${incident.id}`} target="_blank" rel="noreferrer" />}
                  >
                    <Icon name="external-link" />
                    View on {page.name}
                  </Menu.LinkItem>
                ))}
                {isAdmin && (
                  <>
                    <Menu.Separator />
                    <Menu.Item onClick={() => setIsDeleteOpen(true)} className="text-tone-critical-fg">
                      <Icon name="trash" />
                      {maintenance ? 'Delete Maintenance' : 'Delete Incident'}
                    </Menu.Item>
                  </>
                )}
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
        {!closed && scheduled && (
          <Button className="w-fit" disabled={post.isPending} onClick={startNow}>
            <Icon name="play" />
            Start Now
          </Button>
        )}
        {!closed && !scheduled && (
          <Button className="w-fit" onClick={() => setIsResolveOpen(true)}>
            <Icon name="check" />
            {maintenance ? 'Complete' : 'Resolve'}
          </Button>
        )}
      </PageBar>

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-lg:flex-col max-lg:items-center max-lg:px-4">
        <div className="flex w-200 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
              <h1 className="min-w-0 text-title-page font-semibold text-balance">{incident.title}</h1>
              <IncidentStatusBadge status={incident.status} />
            </div>
            <p className="text-body text-text-secondary">{describeSubtitle(incident)}</p>
          </div>

          {maintenance && scheduled && incident.starts_at && incident.ends_at && (
            <p className="flex items-start gap-2 rounded-xl bg-tone-magic-bg px-4 py-3 text-body text-tone-magic-fg">
              <Icon name="calendar" className="mt-0.5 shrink-0" />
              <span>
                <span className="font-medium">
                  Starts {formatCountdown(incident.starts_at)}.
                </span>{' '}
                {incident.components.length > 0
                  ? `Uptime and cron alerts for ${joinNames(incident.components.map((component) => component.name))} are muted from ${formatClock(Date.parse(incident.starts_at))} to ${formatClock(Date.parse(incident.ends_at))}.`
                  : 'Add components to show them Under Maintenance and mute their alerts.'}
              </span>
            </p>
          )}

          <StatStrip columns={6}>
            {maintenance ? (
              <>
                {scheduled && incident.starts_at ? (
                  <Stat label="Starts In" value={formatCountdown(incident.starts_at).replace(/^in /, '')} />
                ) : (
                  <Stat label="Duration" value={formatIncidentDuration(incident)} />
                )}
                <Stat
                  label="Window"
                  value={
                    incident.starts_at && incident.ends_at
                      ? formatSpan((Date.parse(incident.ends_at) - Date.parse(incident.starts_at)) / 1000)
                      : '—'
                  }
                />
              </>
            ) : (
              <>
                <Stat label="Duration" value={formatIncidentDuration(incident)} />
                <Stat label="Severity" value={severity?.label ?? '—'} valueClassName={severity?.textClassName} />
              </>
            )}
            <Stat
              label="Affects"
              value={incident.components.length.toString()}
              detail={incident.components.length === 1 ? 'component' : 'components'}
            />
            <Stat label="Lead" value={incident.lead?.name ?? '—'} />
            <Stat label="Updates" value={incident.public_update_count.toString()} detail="public" />
            <Stat label="Notified" value={incident.channel_ids.length.toString()} detail="channels" />
          </StatStrip>

          {!maintenance && closed ? (
            <PostmortemCard
              key={incident.postmortem_published_at ?? 'draft'}
              incident={incident}
              users={directory.users}
              shownOn={shownOn}
              channelNames={channelNames}
            />
          ) : (
            !closed && (
              <section className="flex flex-col gap-3">
                <h2 className="text-title-section font-medium">Post an Update</h2>
                <IncidentComposer incident={incident} shownOn={shownOn} channelNames={channelNames} />
              </section>
            )
          )}

          <section className="flex flex-col gap-3">
            <div className="flex items-baseline gap-3">
              <h2 className="flex-1 text-title-section font-medium">Timeline</h2>
              <span className="text-body text-text-secondary">Newest first</span>
            </div>
            <IncidentTimeline updates={updates} shownOn={shownOn} onEdit={setEditing} />
          </section>
        </div>

        <aside aria-label="Details" className="flex w-65 shrink-0 flex-col gap-4 pt-2 max-lg:w-200 max-lg:max-w-full">
          <RailSection title="Affected Components" caption="Public">
            {incident.components.length === 0 ? (
              <p className="text-body text-text-secondary">None yet.</p>
            ) : (
              incident.components.map((component) => {
                const status = getComponentStatusMeta(closed ? 'operational' : component.status);
                return (
                  <RailRow key={`${component.kind}-${component.id}`} label={component.name}>
                    {scheduled && incident.starts_at ? (
                      <span className="text-text-secondary">From {formatClock(Date.parse(incident.starts_at))}</span>
                    ) : (
                      <span className={status.className}>{status.label}</span>
                    )}
                  </RailRow>
                );
              })
            )}
            {!closed && (
              <Button variant="secondary" className="w-fit" onClick={() => setIsComponentsOpen(true)}>
                Edit Components
              </Button>
            )}
          </RailSection>
          {maintenance && (
            <RailSection title="Automation">
              <RailRow label="Starts">{incident.auto_start ? 'On its own' : 'Manually'}</RailRow>
              <RailRow label="Finishes">{incident.auto_finish ? 'On its own' : 'Manually'}</RailRow>
              <RailRow label="Reminder">
                {incident.remind_before && incident.starts_at
                  ? formatMoment(Date.parse(incident.starts_at) - 3_600_000)
                  : 'Off'}
              </RailRow>
              <RailRow label="Alerts">Muted in window</RailRow>
            </RailSection>
          )}
          {!maintenance && (
            <RailSection title="Related" caption="Internal">
              {incident.related.length === 0 ? (
                <p className="text-body text-text-secondary">Nothing linked.</p>
              ) : (
                incident.related.map((item) => {
                  const path = getRelatedPath(item);
                  return (
                    <RailRow key={`${item.kind}-${item.id}`} label={RELATED_LABELS[item.kind] ?? item.kind}>
                      {path ? (
                        <Link to={path} className="outline-none hover:underline focus-visible:underline">
                          {item.name}
                        </Link>
                      ) : (
                        item.name
                      )}
                    </RailRow>
                  );
                })
              )}
            </RailSection>
          )}
          <RailSection title="Responders" isLast>
            {incident.lead && <RailRow label={incident.lead.name}>Lead</RailRow>}
            {incident.responders.map((responder) => (
              <RailRow key={responder.id} label={responder.name}>
                {responder.role}
              </RailRow>
            ))}
            {!incident.lead && incident.responders.length === 0 && (
              <p className="text-body text-text-secondary">Nobody yet.</p>
            )}
            <Button variant="secondary" className="w-fit" onClick={() => setIsResponderOpen(true)}>
              Add Responder
            </Button>
          </RailSection>
        </aside>
      </div>

      <ResolveDialog
        incident={incident}
        open={isResolveOpen}
        onOpenChange={setIsResolveOpen}
        shownOn={shownOn}
        channelNames={channelNames}
      />
      <EditUpdateDialog incidentId={incident.id} update={editing} onOpenChange={(open) => !open && setEditing(undefined)} />
      <EditIncidentDialog incident={incident} open={isEditOpen} onOpenChange={setIsEditOpen} />
      <EditComponentsDialog incident={incident} open={isComponentsOpen} onOpenChange={setIsComponentsOpen} />
      <AddResponderDialog incident={incident} open={isResponderOpen} onOpenChange={setIsResponderOpen} />
      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${incident.title}?`}
        description="Its timeline and postmortem go too, and it disappears from your status pages. This cannot be undone."
        confirmLabel={maintenance ? 'Delete Maintenance' : 'Delete Incident'}
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ incidentId: incident.id })}
      />
    </div>
  );
}
