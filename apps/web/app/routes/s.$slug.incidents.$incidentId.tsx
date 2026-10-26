import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { Link, useParams } from 'react-router';

import { DaroyanLogo } from '~/components/brand/daroyan-logo';
import { Icon } from '~/components/icons/icon';
import { IncidentStatusBadge } from '~/components/incidents/incident-badges';
import { RichText } from '~/components/rich-text/rich-text';
import { PublicFooterLinks } from '~/components/status-pages/public-status';
import { Skeleton } from '~/components/ui/skeleton';
import { ApiError } from '~/lib/http';
import { publicIncidentOptions, publicStatusPageOptions } from '~/queries/status-pages';
import { cn } from '~/utils/classname';
import { formatClock, formatDate } from '~/utils/format';
import { formatIncidentDuration, formatWindow, getComponentStatusMeta, getSeverityMeta } from '~/utils/incidents';
import { toDocument } from '~/utils/rich-text';

export function meta() {
  return [{ title: 'Incident' }];
}

function formatStamp(value: string) {
  return `${formatDate(value)}, ${formatClock(Date.parse(value))}`;
}

export default function PublicIncidentRoute() {
  const { slug = '', incidentId = '' } = useParams();
  const { data: page } = useQuery(publicStatusPageOptions(slug));
  const { data: incident, error } = useQuery(publicIncidentOptions(slug, incidentId));

  useEffect(() => {
    if (incident) {
      document.title = page ? `${incident.title} · ${page.name}` : incident.title;
    }
  }, [incident?.title, page?.name]);

  const isMaintenance = incident?.kind === 'maintenance';
  const summary = incident
    ? isMaintenance
      ? [
          'Maintenance',
          incident.starts_at && incident.ends_at ? `${formatWindow(incident.starts_at, incident.ends_at)} your time` : '',
        ]
      : [
          getSeverityMeta(incident.severity)?.label,
          incident.resolved_at
            ? `${formatStamp(incident.started_at)} to ${formatClock(Date.parse(incident.resolved_at))}`
            : `Started ${formatStamp(incident.started_at)}`,
          incident.resolved_at ? `Lasted ${formatIncidentDuration(incident)}` : '',
        ]
    : [];

  return (
    <div className="min-h-dvh bg-surface-page px-4 py-10 text-text-primary max-sm:py-6">
      <div className="mx-auto flex w-full max-w-190 flex-col gap-8">
        <header className="flex flex-col gap-6">
          <div className="flex items-center gap-2.5">
            {page?.logo_url ? (
              <img src={page.logo_url} alt="" className="size-6 shrink-0 rounded-md object-cover" />
            ) : (
              <DaroyanLogo className="size-6" />
            )}
            <span className="min-w-0 flex-1 truncate text-title-card font-medium">{page?.name ?? 'Status'}</span>
          </div>
          <Link
            to={`/s/${slug}`}
            className="flex w-fit items-center gap-1 rounded-xs text-body text-text-link outline-none hover:underline focus-visible:underline"
          >
            <Icon name="chevron-left" className="size-3.5" />
            {page?.name ?? 'Back to status'}
          </Link>
        </header>

        {incident ? (
          <>
            <div className="flex flex-col gap-1.5">
              <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                <h1 className="min-w-0 text-title-page font-semibold text-balance">{incident.title}</h1>
                <IncidentStatusBadge status={incident.status} />
              </div>
              <p className="text-body text-text-secondary">{summary.filter(Boolean).join(' · ')}</p>
            </div>

            <section className="flex flex-col gap-3">
              <div className="flex items-baseline gap-2">
                <h2 className="flex-1 text-title-section font-medium">Updates</h2>
                <span className="text-body text-text-secondary">Newest first</span>
              </div>
              <ol className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
                {incident.updates.length === 0 && (
                  <li className="px-5 py-4 text-body text-text-secondary">No public updates yet.</li>
                )}
                {incident.updates.map((update, index) => (
                  <li key={index} className="flex gap-6 px-5 py-4 max-sm:flex-col max-sm:gap-2">
                    <div className="flex w-28 shrink-0 flex-col items-start gap-1.5 max-sm:w-auto max-sm:flex-row max-sm:items-center">
                      {update.status && <IncidentStatusBadge status={update.status} />}
                      <span className="text-small text-text-secondary">{formatStamp(update.created_at)}</span>
                    </div>
                    <RichText content={toDocument(update.body)} className="flex-1" />
                  </li>
                ))}
              </ol>
            </section>

            {incident.components.length > 0 && (
              <section className="flex flex-col gap-3">
                <div className="flex items-baseline gap-2">
                  <h2 className="flex-1 text-title-section font-medium">Affected Components</h2>
                  <span className="text-body text-text-secondary">During the incident</span>
                </div>
                <ul className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
                  {incident.components.map((component) => {
                    const status = getComponentStatusMeta(component.status);
                    return (
                      <li key={component.name} className="flex items-center gap-3 px-5 py-3 text-body">
                        <span className="min-w-0 flex-1 truncate">{component.name}</span>
                        <span className={cn('shrink-0', status.className)}>{status.label}</span>
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}

            {incident.postmortem && (
              <section className="flex flex-col gap-3">
                <div className="flex items-baseline gap-2">
                  <h2 className="flex-1 text-title-section font-medium">Postmortem</h2>
                  {incident.postmortem_published_at && (
                    <span className="text-body text-text-secondary">
                      Published {formatStamp(incident.postmortem_published_at)}
                    </span>
                  )}
                </div>
                <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
                  <RichText content={toDocument(incident.postmortem)} className="px-5 py-4" />
                  {incident.follow_ups.length > 0 && (
                    <div className="flex flex-col gap-1.5 px-5 py-4">
                      <h3 className="text-body font-medium">Follow-Ups</h3>
                      <ul className="list-disc pl-5 text-body text-text-secondary">
                        {incident.follow_ups.map((followUp, index) => (
                          <li key={index}>
                            {followUp.body}
                            {followUp.is_done && <span className="font-semibold text-text-primary"> (done)</span>}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </div>
              </section>
            )}
          </>
        ) : error ? (
          <div className="flex flex-col items-center gap-1.5 pt-10 text-center">
            <p className="text-title-section font-medium">
              {error instanceof ApiError && error.status === 404 ? 'Incident Not Found' : 'Something Went Wrong'}
            </p>
            <p className="text-body text-text-secondary">
              {error instanceof ApiError && error.status === 404
                ? 'This incident is not on this status page.'
                : 'The incident could not load. Try again in a moment.'}
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-6">
            <Skeleton size="title" className="w-96" />
            <Skeleton size="block" className="h-60 rounded-2xl" />
          </div>
        )}

        <footer className="flex items-center justify-center text-caption font-normal">
          <PublicFooterLinks slug={slug} />
        </footer>
      </div>
    </div>
  );
}
