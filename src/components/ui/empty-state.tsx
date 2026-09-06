import * as React from "react";
import type { LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

/**
 * EmptyState — placeholder para cuando no hay datos. `tone` solo
 * colorea icono y título; si hay CTA, va con Button secundario.
 */
export type EmptyStateTone = "default" | "info" | "warning" | "error";

export interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  subtitle?: string;
  ctaText?: string;
  onCtaClick?: () => void;
  tone?: EmptyStateTone;
  className?: string;
}

const TONE_ICON: Record<EmptyStateTone, string> = {
  default: "text-muted-foreground/60",
  info: "text-muted-foreground/60",
  warning: "text-warning",
  error: "text-error",
};

const TONE_TITLE: Record<EmptyStateTone, string> = {
  default: "text-muted-foreground",
  info: "text-muted-foreground",
  warning: "text-warning-foreground",
  error: "text-error-foreground",
};

export const EmptyState: React.FC<EmptyStateProps> = ({
  icon: Icon,
  title,
  subtitle,
  ctaText,
  onCtaClick,
  tone = "default",
  className,
}) => {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center px-6 py-10 text-center",
        className,
      )}
    >
      {Icon ? (
        <div className="flex h-10 w-10 items-center justify-center rounded-full bg-muted">
          <Icon className={cn("h-5 w-5", TONE_ICON[tone])} aria-hidden="true" />
        </div>
      ) : null}
      <p className={cn("mt-3 text-sm font-medium", TONE_TITLE[tone])}>
        {title}
      </p>
      {subtitle ? (
        <p className="mt-1 max-w-sm text-xs leading-relaxed text-muted-foreground/80">
          {subtitle}
        </p>
      ) : null}
      {ctaText && onCtaClick ? (
        <Button
          variant="secondary"
          size="md"
          className="mt-4"
          onClick={onCtaClick}
        >
          {ctaText}
        </Button>
      ) : null}
    </div>
  );
};
EmptyState.displayName = "EmptyState";
