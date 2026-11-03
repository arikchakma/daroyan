import { useQuery } from '@tanstack/react-query';
import { useParams } from 'react-router';

import { IssueDetail } from '~/components/issues/issue-detail';
import { issueQueries } from '~/queries/traces';

export function meta() {
  return [{ title: 'Issue · Daroyan' }];
}

export default function IssuePage() {
  const { fingerprint = '' } = useParams();
  const { data: issues } = useQuery(issueQueries.list(0));
  const { data: events = [], isLoading: isEventsLoading } = useQuery(issueQueries.events(fingerprint));
  const issue = issues?.find((item) => item.fingerprint === fingerprint);

  if (!issues) {
    return null;
  }

  if (!issue) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Issue Not Found</p>
        <p className="text-body text-text-secondary">It may be older than the retention window.</p>
      </div>
    );
  }

  return <IssueDetail issue={issue} events={events} isEventsLoading={isEventsLoading} />;
}
