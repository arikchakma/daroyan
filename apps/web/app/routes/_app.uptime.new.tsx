import { CheckForm } from '~/components/uptime/check-form';

export function meta() {
  return [{ title: 'New Check · Daroyan' }];
}

export default function NewCheckPage() {
  return <CheckForm />;
}
