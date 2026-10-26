import { StatusPageForm } from '~/components/status-pages/status-page-form';

export function meta() {
  return [{ title: 'New Status Page · Daroyan' }];
}

export default function NewStatusPagePage() {
  return <StatusPageForm />;
}
