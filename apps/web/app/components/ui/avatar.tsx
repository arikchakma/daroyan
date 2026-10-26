import { getAvatarPaths, AVATAR_GRID, pickPalette } from '~/utils/avatar';
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

  const paths = getAvatarPaths(id, pickPalette(id));

  return (
    <svg
      viewBox={`0 0 ${AVATAR_GRID} ${AVATAR_GRID}`}
      aria-hidden="true"
      shapeRendering="crispEdges"
      className={cn(
        'size-5.5 shrink-0',
        shape === 'square' ? 'rounded-xs' : 'rounded-full',
        className
      )}
    >
      {paths.map((path) => (
        <path key={path.fill} d={path.d} fill={path.fill} />
      ))}
    </svg>
  );
}
