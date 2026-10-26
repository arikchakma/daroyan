import { Tabs as TabsPrimitive } from "@base-ui/react/tabs";

import { cn } from "~/utils/classname";

function TabsList(props: TabsPrimitive.List.Props) {
  const { className, ...rest } = props;

  return (
    <TabsPrimitive.List
      className={cn(
        "flex items-start gap-5 border-b border-border-row px-4",
        className,
      )}
      {...rest}
    />
  );
}

function TabsTab(props: TabsPrimitive.Tab.Props) {
  const { className, ...rest } = props;

  return (
    <TabsPrimitive.Tab
      className={cn(
        "-mb-px flex shrink-0 items-center gap-1.5 border-b-2 border-transparent pt-3 pb-2.5 text-body font-medium whitespace-nowrap text-text-secondary outline-none hover:text-text-primary focus-visible:text-text-primary focus-visible:underline data-active:border-button-primary data-active:text-text-primary data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:size-3.5",
        className,
      )}
      {...rest}
    />
  );
}

function TabsPanel(props: TabsPrimitive.Panel.Props) {
  const { className, ...rest } = props;

  return (
    <TabsPrimitive.Panel className={cn("outline-none", className)} {...rest} />
  );
}

export const Tabs = {
  Root: TabsPrimitive.Root,
  List: TabsList,
  Tab: TabsTab,
  Panel: TabsPanel,
};
