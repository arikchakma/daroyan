import { useEffect } from 'react';
import { redirect, useSearchParams } from 'react-router';

import { DaroyanLogo } from '~/components/brand/daroyan-logo';
import { GoogleLoginButton } from '~/components/authentication/google-login-button';
import { isLoggedIn } from '~/lib/auth';
import { toast } from '~/lib/toast';

export function meta() {
  return [{ title: 'Sign In · Daroyan' }, { name: 'robots', content: 'noindex' }];
}

export function clientLoader() {
  if (isLoggedIn()) {
    throw redirect('/');
  }
  return null;
}

export default function LoginPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const signInError = searchParams.get('error');
  const next = searchParams.get('next') ?? undefined;

  useEffect(() => {
    if (!signInError) {
      return;
    }
    toast.error(signInError, { id: 'sign-in-error' });
    setSearchParams(
      (params) => {
        params.delete('error');
        return params;
      },
      { replace: true }
    );
  }, [signInError]);

  return (
    <div className="flex min-h-dvh items-center justify-center bg-surface-page px-6 text-text-primary">
      <div className="flex w-full max-w-90 flex-col items-center gap-6 text-center">
        <div className="flex flex-col items-center gap-5">
          <div className="flex items-center gap-2.5">
            <DaroyanLogo className="size-7" />
            <span className="text-title-card font-semibold">Daroyan</span>
          </div>
          <div className="flex flex-col gap-1.5">
            <h1 className="text-title-auth font-semibold">Welcome Back</h1>
            <p className="text-body text-text-secondary">Sign in to Daroyan with your Google account.</p>
          </div>
        </div>
        <div className="flex w-full flex-col">
          <GoogleLoginButton next={next} />
        </div>
      </div>
    </div>
  );
}
