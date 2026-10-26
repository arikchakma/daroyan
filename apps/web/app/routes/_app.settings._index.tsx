import { redirect } from 'react-router';

export function clientLoader() {
  throw redirect('/settings/integrations');
}

export default function SettingsIndex() {
  return null;
}
