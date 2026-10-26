import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { getSeverityMeta, getStatusMeta } from '~/utils/incidents';

type IncidentStatusBadgeProps = {
  status: string;
  hasIcon?: boolean;
};

export function IncidentStatusBadge(props: IncidentStatusBadgeProps) {
  const { status, hasIcon = true } = props;

  const meta = getStatusMeta(status);

  return (
    <Badge tone={meta.tone}>
      {hasIcon && <Icon name={meta.icon} />}
      {meta.label}
    </Badge>
  );
}

type SeverityBadgeProps = {
  severity: string | null | undefined;
};

export function SeverityBadge(props: SeverityBadgeProps) {
  const { severity } = props;

  const meta = getSeverityMeta(severity);
  if (!meta) {
    return null;
  }

  return <Badge tone={meta.tone}>{meta.label}</Badge>;
}
