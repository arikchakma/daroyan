import { Icon } from '~/components/icons/icon';
import { INTEGRATION_KINDS, isIntegrationKind } from '~/utils/alerts';

type IntegrationLogoProps = {
  kind: string;
};

export function IntegrationLogo(props: IntegrationLogoProps) {
  const { kind } = props;

  return (
    <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-surface-fill">
      <Icon name={isIntegrationKind(kind) ? INTEGRATION_KINDS[kind].icon : 'webhook'} />
    </span>
  );
}

export function getIntegrationLabel(kind: string) {
  return isIntegrationKind(kind) ? INTEGRATION_KINDS[kind].label : kind;
}
