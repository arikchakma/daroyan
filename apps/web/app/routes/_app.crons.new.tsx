import { CronForm } from '~/components/crons/cron-form';

export function meta() {
  return [{ title: 'New Cron · Daroyan' }];
}

export default function NewCronPage() {
  return <CronForm />;
}
