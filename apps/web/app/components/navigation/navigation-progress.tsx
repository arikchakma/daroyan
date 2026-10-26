import NProgress from 'nprogress';

import 'nprogress/nprogress.css';
import { useEffect } from 'react';
import { useNavigation } from 'react-router';

NProgress.configure({
  minimum: 0.1,
  showSpinner: false,
  trickleSpeed: 200,
  easing: 'ease',
  speed: 400,
});

export function NavigationProgress() {
  const navigation = useNavigation();
  const state = navigation.state;

  useEffect(() => {
    if (state === 'idle') {
      NProgress.done();
      return;
    }

    NProgress.start();
  }, [state]);

  return null;
}
