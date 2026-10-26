import { useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router';

import { CONNECTED_MESSAGE } from '~/utils/alerts';

export function meta() {
  return [{ title: 'Connected · Daroyan' }];
}

export default function IntegrationConnectedPage() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const integrationId = searchParams.get('integration_id');
  const error = searchParams.get('error');

  useEffect(() => {
    if (window.opener) {
      window.opener.postMessage({ type: CONNECTED_MESSAGE, integrationId, error }, window.location.origin);
      window.close();
      return;
    }
    const params = new URLSearchParams(integrationId ? { connected: integrationId } : { error: error ?? '' });
    navigate(`/settings/integrations?${params}`, { replace: true });
  }, [integrationId, error, navigate]);

  return (
    <div className="flex h-dvh items-center justify-center bg-surface-default p-6 text-center text-body text-text-secondary">
      {error ?? 'Connected. You can close this window.'}
    </div>
  );
}
