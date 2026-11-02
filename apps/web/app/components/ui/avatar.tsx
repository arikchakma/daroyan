import { DitheringAvatar } from 'dithering-avatar';
import { cn } from '~/utils/classname';

type AvatarProps = {
  id: string;
  src?: string | null;
  shape?: 'square' | 'circle';
  className?: string;
};

export function Avatar(props: AvatarProps) {
  const { id, src, shape = 'circle', className } = props;

  if (src) {
    return (
      <img
        src={src}
        alt=""
        referrerPolicy="no-referrer"
        className={cn(
          'size-5.5 shrink-0 object-cover',
          shape === 'square' ? 'rounded-xs' : 'rounded-full',
          className
        )}
      />
    );
  }

  return (
    <DitheringAvatar
      name={id}
      style={{ borderRadius: undefined }}
      className={cn(
        'size-5.5 shrink-0',
        shape === 'square' ? 'rounded-xs' : 'rounded-full',
        className
      )}
    />
  );
}
