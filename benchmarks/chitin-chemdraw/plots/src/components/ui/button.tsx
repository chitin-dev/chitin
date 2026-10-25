import { Button as Primitive } from "@kobalte/core/button";
import { cva, type VariantProps } from "class-variance-authority";
import { splitProps, type ComponentProps } from "solid-js";
import { cn } from "@/lib/utils";

export const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary/90",
        outline: "border border-input bg-background hover:bg-accent hover:text-accent-foreground",
        ghost: "hover:bg-accent hover:text-accent-foreground",
      },
      size: { default: "h-9 px-3", sm: "h-8 px-2.5 text-xs", icon: "size-8" },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);
/** Kobalte owns keyboard activation and disabled semantics; styling stays local. */
export function Button(props: ComponentProps<typeof Primitive> & VariantProps<typeof buttonVariants>) {
  const [local, rest] = splitProps(props, ["class", "variant", "size"]);
  return <Primitive class={cn(buttonVariants({ variant: local.variant, size: local.size }), local.class)} {...rest} />;
}
