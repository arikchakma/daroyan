import { Icon } from '~/components/icons/icon';

type FilterAddButtonProps = {
  onClick: () => void;
};

export function FilterAddButton(props: FilterAddButtonProps) {
  const { onClick } = props;

  return (
    <button
      type="button"
      aria-label="Add Filter"
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      className="ml-0.5 inline-flex size-5 items-center justify-center rounded-full align-middle text-text-tertiary outline-none hover:text-text-secondary"
    >
      <Icon name="circle-plus" />
    </button>
  );
}
