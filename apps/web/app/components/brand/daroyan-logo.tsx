import { cn } from '~/utils/classname';

type DaroyanLogoProps = {
  className?: string;
};

export function DaroyanLogo(props: DaroyanLogoProps) {
  const { className } = props;

  return <img src="/icon.svg" alt="" draggable={false} className={cn('size-5 shrink-0 select-none', className)} />;
}
