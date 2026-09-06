import * as React from "react";
import { ImagePlus, FolderOpen, Sparkles } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { DropdownMenu, type DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { isTauriEnvironment } from "@/lib/tauri-safe";

/**
 * DropZone — zona de arrastrar/soltar archivos.
 *
 * En Tauri los drops van por onDragDropEvent del webview (los eventos
 * HTML5 no exponen el path real del archivo); en navegador el drop
 * solo avisa: los File del dataTransfer no tienen path de disco.
 */
export interface DropZoneProps {
  title?: string;
  subtitle?: string;
  buttonText?: string;
  onPathsDropped: (paths: string[]) => void;
  onBrowse?: () => void;
  className?: string;
  menuItems?: DropdownMenuItem[];
  menuSelectedId?: string;
  onMenuSelect?: (id: string) => void;
  onMenuTrigger?: (id: string) => void;
  menuTriggerLabel?: string;
  /** Variante compacta: layout horizontal en vez de centrado. */
  compact?: boolean;
}

const IMAGE_EXTENSIONS = [
  ".png",
  ".jpg",
  ".jpeg",
  ".webp",
  ".avif",
  ".gif",
  ".bmp",
  ".tif",
  ".tiff",
];

function isImagePath(path: string): boolean {
  const lower = path.toLowerCase();
  return IMAGE_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

export const DropZone: React.FC<DropZoneProps> = ({
  title = "Arrastra imágenes aquí",
  subtitle = "PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF",
  buttonText = "Explorar archivos",
  onPathsDropped,
  onBrowse,
  className,
  menuItems,
  menuSelectedId,
  onMenuSelect,
  onMenuTrigger,
  menuTriggerLabel = "Añadir",
  compact = false,
}) => {
  const [dragOver, setDragOver] = React.useState(false);
  const [dragInside, setDragInside] = React.useState(false);

  // El listener del webview se suscribe una sola vez; el ref evita
  // capturar un callback desactualizado.
  const onPathsDroppedRef = React.useRef(onPathsDropped);
  onPathsDroppedRef.current = onPathsDropped;

  React.useEffect(() => {
    if (!isTauriEnvironment()) return;
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const u = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "enter") {
            setDragInside(true);
            setDragOver(true);
          } else if (event.payload.type === "over") {
            setDragInside(true);
            setDragOver(true);
          } else if (event.payload.type === "leave") {
            setDragInside(false);
            setDragOver(false);
          } else if (event.payload.type === "drop") {
            setDragInside(false);
            setDragOver(false);
            const paths = event.payload.paths;
            const filtered = paths.filter(
              (p) => !p.includes(".") || isImagePath(p),
            );
            if (filtered.length > 0) {
              onPathsDroppedRef.current(filtered);
            }
          }
        });
        if (cancelled) {
          u();
        } else {
          unlisten = u;
        }
      } catch (e) {
        console.warn("onDragDropEvent no disponible:", e);
      }
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  const handleDragOver = (e: React.DragEvent) => {
    if (isTauriEnvironment()) return;
    e.preventDefault();
    e.stopPropagation();
    setDragOver(true);
  };
  const handleDragLeave = (e: React.DragEvent) => {
    if (isTauriEnvironment()) return;
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
  };
  const handleDrop = (e: React.DragEvent) => {
    if (isTauriEnvironment()) return;
    e.preventDefault();
    e.stopPropagation();
    setDragOver(false);
    const files = Array.from(e.dataTransfer.files);
    if (files.length > 0) {
      const names = files.map((f) => f.name).join(", ");
      console.warn(
        `Drop en navegador: los archivos (${names}) no tienen path. ` +
          "Ejecuta la app en modo escritorio (pnpm tauri:dev) para procesarlos.",
      );
    }
  };

  const handleMenuSelect = (id: string) => {
    onMenuSelect?.(id);
    onMenuTrigger?.(id);
  };

  const active = dragOver && dragInside;

  if (compact) {
    return (
      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        className={cn(
          "flex items-center gap-3 rounded-md border px-3 py-2 transition-colors duration-fast",
          active
            ? "border-primary bg-primary-subtle"
            : "border-border bg-surface-inset hover:border-border-strong",
          className,
        )}
      >
        <ImagePlus
          className={cn(
            "h-4 w-4 shrink-0",
            active ? "text-primary" : "text-muted-foreground",
          )}
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">
            {title}
          </p>
          <p className="truncate text-xs text-muted-foreground">{subtitle}</p>
        </div>
        {menuItems && menuItems.length > 0 ? (
          <DropdownMenu
            triggerLabel={menuTriggerLabel}
            triggerIcon={<FolderOpen className="h-4 w-4" aria-hidden="true" />}
            variant="secondary"
            size="md"
            items={menuItems}
            selectedId={menuSelectedId}
            onSelect={handleMenuSelect}
          />
        ) : (
          <Button variant="secondary" size="md" onClick={onBrowse} type="button">
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {buttonText}
          </Button>
        )}
      </div>
    );
  }

  return (
    <div
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
      className={cn(
        "flex flex-col items-center justify-center rounded-lg border border-dashed px-8 py-12 text-center transition-colors duration-fast",
        active
          ? "border-primary bg-primary-subtle"
          : "border-border bg-surface-sunken hover:border-border-strong hover:bg-surface-inset",
        className,
      )}
    >
      <div
        className={cn(
          "flex h-14 w-14 items-center justify-center rounded-full transition-colors",
          active ? "bg-primary/10" : "bg-muted",
        )}
      >
        <ImagePlus
          className={cn(
            "h-6 w-6",
            active ? "text-primary" : "text-muted-foreground",
          )}
          aria-hidden="true"
        />
      </div>
      <p className="mt-4 text-base font-medium text-foreground">{title}</p>
      <p className="mt-1 text-sm text-muted-foreground">{subtitle}</p>
      {!isTauriEnvironment() ? (
        <p className="mt-2 text-xs text-warning-foreground">
          El arrastrado requiere la app de escritorio. Usa el botón para
          navegar en modo navegador.
        </p>
      ) : null}
      <div className="mt-5">
        {menuItems && menuItems.length > 0 ? (
          <DropdownMenu
            triggerLabel={menuTriggerLabel}
            triggerIcon={<Sparkles className="h-4 w-4" aria-hidden="true" />}
            variant="primary"
            size="md"
            items={menuItems}
            selectedId={menuSelectedId}
            onSelect={handleMenuSelect}
          />
        ) : (
          <Button variant="secondary" size="md" onClick={onBrowse} type="button">
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {buttonText}
          </Button>
        )}
      </div>
    </div>
  );
};
DropZone.displayName = "DropZone";
