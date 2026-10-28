import type { Schemas } from '@daroyan/client';
import { useState } from 'react';
import { Link } from 'react-router';

import { DaroyanLogo } from '~/components/brand/daroyan-logo';
import { Icon } from '~/components/icons/icon';
import { IncidentStatusBadge } from '~/components/incidents/incident-badges';
import { RichText } from '~/components/rich-text/rich-text';
import { Chip } from '~/components/ui/chip';
import { Tooltip } from '~/components/ui/tooltip';
import { cn } from '~/utils/classname';
import { DAY_MS, formatClock, formatDate, formatRelative } from '~/utils/format';
import { formatIncidentDuration, formatWindow, getStatusMeta } from '~/utils/incidents';
import { toDocument } from '~/utils/rich-text';
import {
  describeDayIssues,
  getComponentStatus,
  getDayClassName,
  summarizeComponents,
} from '~/utils/status-pages';
import { formatUptime } from '~/utils/uptime';

const PHONE_DAYS = 30;
const HISTORY_DAYS = 14;
const DAY_PREVIEW = 3;

const SEVERITY_CLASS_NAMES: Record<string, string> = {
  critical: 'text-tone-critical-fg',
  major: 'text-tone-warning-fg',
  minor: 'text-text-primary',
};

const BANNERS = {
  operational: {
    title: 'All systems operational',
    icon: 'circle-check',
    className: 'border-tone-success-bg bg-tone-success-bg/40',
    titleClassName: 'text-tone-success-fg',
  },
  partial: {
    title: 'Partial outage',
    icon: 'triangle-alert',
    className: 'border-tone-critical-bg bg-tone-critical-surface',
    titleClassName: 'text-tone-critical-fg',
  },
  major: {
    title: 'Major outage',
    icon: 'circle-x',
    className: 'border-tone-critical-bg bg-tone-critical-surface',
    titleClassName: 'text-tone-critical-fg',
  },
  maintenance: {
    title: 'Under maintenance',
    icon: 'calendar',
    className: 'border-tone-info-bg bg-tone-info-bg/40',
    titleClassName: 'text-tone-info-fg',
  },
} as const;

type PublicStatusProps = {
  slug: string;
  page: Schemas['PublicStatusPage'];
};

export function PublicStatus(props: PublicStatusProps) {
  const { slug, page } = props;

  const banner = BANNERS[page.status as keyof typeof BANNERS] ?? BANNERS.operational;
  const components = page.sections.flatMap((section) => section.components);
  const timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;

  return (
    <div className="mx-auto flex w-full max-w-190 flex-col gap-8">
      <header className="flex items-center gap-2.5">
        {page.logo_url ? (
          <img src={page.logo_url} alt="" className="size-6 shrink-0 rounded-md object-cover" />
        ) : (
          <DaroyanLogo className="size-6" />
        )}
        <h1 className="min-w-0 flex-1 truncate text-title-card font-medium text-text-primary">{page.name}</h1>
      </header>

      <div className={cn('flex items-start gap-3 rounded-xl border px-4 py-3.5', banner.className)}>
        <Icon name={banner.icon} className={cn('mt-0.5', banner.titleClassName)} />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <p className={cn('text-body font-medium', banner.titleClassName)}>{banner.title}</p>
          {page.status !== 'operational' && (
            <p className="text-body text-text-primary">{summarizeComponents(components)}</p>
          )}
        </div>
        <p className="shrink-0 text-body text-text-secondary max-sm:hidden">Updated {formatRelative(page.updated_at)}</p>
      </div>

      {page.active_incidents.map((incident) => (
        <EventCard
          key={incident.id}
          slug={slug}
          id={incident.id}
          title={incident.title}
          status={incident.latest_update?.status ?? incident.status}
          body={incident.latest_update?.body}
          meta={[
            incident.latest_update ? `Updated ${formatClock(Date.parse(incident.latest_update.created_at))}` : undefined,
            `Started ${formatClock(Date.parse(incident.started_at))}`,
          ]}
          components={incident.components.map((component) => component.name)}
          hasLink
        />
      ))}

      {page.maintenance.length > 0 && (
        <section className="flex flex-col gap-3">
          <h2 className="text-title-section font-medium text-text-primary">
            {page.maintenance.some((item) => item.status === 'in_progress') ? 'Maintenance' : 'Upcoming Maintenance'}
          </h2>
          {page.maintenance.map((item) => (
            <EventCard
              key={item.id}
              slug={slug}
              id={item.id}
              title={item.title}
              status={item.status}
              body={item.latest_update?.body}
              meta={[item.starts_at && item.ends_at ? `${formatWindow(item.starts_at, item.ends_at)} your time` : undefined]}
              components={item.components.map((component) => component.name)}
              isStatusRight
            />
          ))}
        </section>
      )}

      {page.sections.map((section, index) => (
        <section key={index} className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <h2 className="min-w-0 flex-1 truncate text-title-section font-medium text-text-primary">
              {section.name}
            </h2>
            <p className="shrink-0 text-body text-text-secondary">
              <span className="max-sm:hidden">Last 90 days</span>
              <span className="sm:hidden">Last {PHONE_DAYS} days</span>
            </p>
          </div>
          <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
            {section.components.map((component, componentIndex) => (
              <ComponentRow key={componentIndex} component={component} />
            ))}
          </div>
        </section>
      ))}

      <PastIncidents slug={slug} incidents={page.past_incidents} />

      <footer className="flex items-center justify-between gap-4 text-caption font-normal">
        <p className="text-text-tertiary">Times are in {timeZone.replace(/_/g, ' ')}.</p>
        <PublicFooterLinks slug={slug} />
      </footer>
    </div>
  );
}

type ComponentRowProps = {
  component: Schemas['PublicStatusComponent'];
};

function ComponentRow(props: ComponentRowProps) {
  const { component } = props;

  const status = getComponentStatus(component.status);
  const today = component.days.at(-1)?.day;
  const phoneStart = component.days.length - PHONE_DAYS;

  return (
    <div className="flex flex-col gap-2.5 px-5 py-4">
      <div className="flex items-center gap-2 text-body font-medium">
        <p className="min-w-0 flex-1 truncate text-text-primary">{component.name}</p>
        <p className={cn('shrink-0', status.className)}>{status.label}</p>
      </div>
      <Tooltip.Provider delay={0} closeDelay={0}>
        <div className="flex h-7 items-stretch gap-0.5">
          {component.days.map((day, index) => {
            const issues = describeDayIssues(day, component.kind);
            const label = day.day === today ? 'Today' : formatDate(day.day);
            const uptime = day.total > 0 ? `${formatUptime((day.total - day.down) / day.total)} uptime` : 'No data';
            return (
              <Tooltip.Root key={day.day}>
                <Tooltip.Trigger
                  render={<span />}
                  aria-label={`${label}: ${uptime}`}
                  className={cn(
                    'min-w-0.5 flex-1 rounded-[1.5px] hover:opacity-70',
                    getDayClassName(day),
                    index < phoneStart && 'max-sm:hidden'
                  )}
                />
                <Tooltip.Portal>
                  <Tooltip.Positioner side="top">
                    <Tooltip.Popup className="flex-col items-start gap-0.5">
                      <span>{label}</span>
                      <span className="font-normal opacity-80">{uptime}</span>
                      {issues && <span className="font-normal opacity-80">{issues}</span>}
                      <Tooltip.Arrow />
                    </Tooltip.Popup>
                  </Tooltip.Positioner>
                </Tooltip.Portal>
              </Tooltip.Root>
            );
          })}
        </div>
      </Tooltip.Provider>
      <div className="flex items-start justify-between text-axis">
        <p className="text-text-tertiary">
          <span className="max-sm:hidden">90 days ago</span>
          <span className="sm:hidden">{PHONE_DAYS} days ago</span>
        </p>
        <p className="text-text-secondary">{formatUptime(component.uptime)} uptime</p>
        <p className="text-text-tertiary">Today</p>
      </div>
    </div>
  );
}

export function PublicFooterLinks(props: { slug: string }) {
  const { slug } = props;

  return (
    <p className="flex shrink-0 items-center gap-1.5 text-text-secondary">
      <DaroyanLogo className="size-3.5" />
      Powered by Daroyan
      <span aria-hidden="true">·</span>
      <a
        href={`/v1/status/${slug}/feed.rss`}
        className="flex items-center gap-1 rounded-xs text-text-link outline-none hover:underline focus-visible:underline"
      >
        <Icon name="link" className="size-3.5" />
        RSS Feed
      </a>
    </p>
  );
}

type EventCardProps = {
  slug: string;
  id: string;
  title: string;
  status: string;
  body: unknown;
  meta: (string | undefined)[];
  components: string[];
  hasLink?: boolean;
  isStatusRight?: boolean;
};

function EventCard(props: EventCardProps) {
  const { slug, id, title, status, body, meta, components, hasLink = false, isStatusRight = false } = props;

  return (
    <article className="flex flex-col gap-2 rounded-2xl border border-border-ring bg-surface-default px-4 py-3.5 shadow-card">
      <div className="flex items-start gap-3">
        <h3 className="min-w-0 flex-1 text-body font-medium text-text-primary">
          <Link
            to={`/s/${slug}/incidents/${id}`}
            className="outline-none hover:underline focus-visible:underline"
          >
            {title}
          </Link>
        </h3>
        {hasLink && (
          <Link
            to={`/s/${slug}/incidents/${id}`}
            className="shrink-0 rounded-xs text-body text-text-link outline-none hover:underline focus-visible:underline"
          >
            View Incident
          </Link>
        )}
        {isStatusRight && <IncidentStatusBadge status={status} />}
      </div>
      <div className="flex min-w-0 items-start gap-2">
        {!isStatusRight && (
          <span className="shrink-0">
            <IncidentStatusBadge status={status} />
          </span>
        )}
        <RichText content={toDocument(body)} className="min-w-0 flex-1 pt-px" />
      </div>
      <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 text-small text-text-secondary">
        {[...meta.filter(Boolean), components.length > 0 ? 'Affects' : undefined].filter(Boolean).join(' · ')}
        {components.map((name) => (
          <Chip key={name}>{name}</Chip>
        ))}
      </p>
    </article>
  );
}

type PastIncidentsProps = {
  slug: string;
  incidents: Schemas['PublicStatusPastIncident'][];
};

function PastIncidents(props: PastIncidentsProps) {
  const { slug, incidents } = props;

  const [isExpanded, setIsExpanded] = useState(false);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const days = Array.from({ length: HISTORY_DAYS }, (_, index) => {
    const start = today.getTime() - index * DAY_MS;
    return {
      start,
      incidents: incidents.filter((incident) => {
        const started = Date.parse(incident.started_at);
        return started >= start && started < start + DAY_MS;
      }),
    };
  });
  const visible = isExpanded ? days : days.slice(0, DAY_PREVIEW);
  const hidden = days.length - visible.length;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-baseline gap-2">
        <h2 className="flex-1 text-title-section font-medium text-text-primary">Past Incidents</h2>
        <p className="text-body text-text-secondary">Last {HISTORY_DAYS} days</p>
      </div>
      <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        {visible.map((day) => (
          <div key={day.start} className="flex flex-col gap-2 px-4 py-3.5">
            <p className="text-body font-medium text-text-primary">{formatDate(day.start)}</p>
            {day.incidents.length === 0 ? (
              <p className="text-small text-text-secondary">No incidents reported.</p>
            ) : (
              day.incidents.map((incident) => (
                <div key={incident.id} className="flex flex-col gap-0.5">
                  <Link
                    to={`/s/${slug}/incidents/${incident.id}`}
                    className={cn(
                      'w-fit text-body outline-none hover:underline focus-visible:underline',
                      SEVERITY_CLASS_NAMES[incident.severity ?? 'minor']
                    )}
                  >
                    {incident.title}
                  </Link>
                  <p className="text-small text-text-secondary">
                    {getStatusMeta(incident.status).label} ·{' '}
                    {incident.resolved_at
                      ? `Lasted ${formatIncidentDuration(incident)}`
                      : `Started ${formatClock(Date.parse(incident.started_at))}`}
                  </p>
                </div>
              ))
            )}
          </div>
        ))}
        {hidden > 0 && (
          <div className="px-4 py-3">
            <button
              type="button"
              onClick={() => setIsExpanded(true)}
              className="rounded-xs text-body text-text-link outline-none hover:underline focus-visible:underline"
            >
              + {hidden} more days
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
