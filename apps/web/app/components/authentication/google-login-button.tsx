import { useMutation } from '@tanstack/react-query';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { toast } from '~/lib/toast';
import { googleLoginOptions } from '~/queries/auth';

type GoogleLoginButtonProps = {
  next?: string;
};

export function GoogleLoginButton(props: GoogleLoginButtonProps) {
  const { next } = props;

  const { mutate: googleLogin, isPending } = useMutation({
    ...googleLoginOptions(),
    meta: { isSilent: true },
    onSuccess: (data) => {
      window.location.href = data.authorization_url;
    },
    onError: (error) => toast.error(error.message),
  });

  return (
    <Button type="button" variant="secondary" size="lg" disabled={isPending} onClick={() => googleLogin(next)}>
      {isPending ? <Icon name="loader" className="animate-spin" /> : <Icon name="logo-google" />}
      Continue with Google
    </Button>
  );
}
