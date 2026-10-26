import { redirect } from 'react-router';

export function clientLoader() {
  throw redirect('/');
}

export default function OverviewRedirect() {
  return null;
}
