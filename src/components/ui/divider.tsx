import * as React from "react";

import { cn } from "@/lib/utils";

/** Divider — separador horizontal o vertical. */
export interface DividerProps {
  vertical?: boolean;
  className?: string;
}

export const Divider: React.FC<DividerProps> = ({ vertical, className }) => {
  return (
    <div
      role="separator"
      aria-orientation={vertical ? "vertical" : "horizontal"}
      className={cn(
        vertical ? "h-full w-px bg-border" : "h-px w-full bg-border",
        className,
      )}
    />
  );
};
Divider.displayName = "Divider";
