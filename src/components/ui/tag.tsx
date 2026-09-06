import * as React from "react";

import { cn } from "@/lib/utils";

/** Tag — etiqueta pequeña para metadata o estado. */
export type TagTone =
  | "neutral"
  | "primary"
  | "success"
  | "warning"
  | "error"
  | "info";

export interface TagProps extends React.HTMLAttributes<HTMLSpanElement> {
  size?: "sm" | "md";
  tone?: TagTone;
}

const TONE_CLASSES: Record<TagTone, string> = {
  neutral: "bg-muted text-muted-foreground",
  primary: "bg-primary-subtle text-foreground",
  success: "bg-success-subtle text-success-foreground",
  warning: "bg-warning-subtle text-warning-foreground",
  error: "bg-error-subtle text-error-foreground",
  info: "bg-info-subtle text-info-foreground",
};

export const Tag: React.FC<TagProps> = ({
  className,
  size = "sm",
  tone = "neutral",
  ...props
}) => {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-xs font-medium",
        size === "sm" ? "h-[18px] px-1.5 text-2xs" : "h-5 px-2 text-xs",
        TONE_CLASSES[tone],
        className,
      )}
      {...props}
    />
  );
};
Tag.displayName = "Tag";
