import * as React from "react";
import { Boxes, Cpu, Brain, Shield } from "lucide-react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Divider } from "@/components/ui/divider";
import { useEngine } from "@/state/engine-context";
import * as tauri from "@/services/tauri";

export const AboutPage: React.FC = () => {
  const { info } = useEngine();
  const [backends, setBackends] = React.useState<Array<[string, string]>>([]);
  const [tier, setTier] = React.useState<string>("Community");
  const [status, setStatus] = React.useState<string>("—");

  React.useEffect(() => {
    void tauri
      .listIntelBackends()
      .then(setBackends)
      .catch(() => setBackends([]));
    void tauri
      .getLicenseTier()
      .then((t) => setTier(t))
      .catch(() => setTier("Community"));
    void tauri
      .getLicenseStatus()
      .then((s) => setStatus(s === "Unknown" ? "—" : s))
      .catch(() => setStatus("—"));
  }, []);

  return (
    <div className="mx-auto max-w-3xl">
      <Card>
        <CardHeader title="Acerca de BoxFlux" />
        <CardContent className="flex flex-col gap-6">
          {/* Brand hero */}
          <div className="flex items-center gap-4">
            <div className="flex h-14 w-14 items-center justify-center rounded-lg bg-primary text-primary-foreground shadow-md">
              <Boxes className="h-7 w-7" aria-hidden="true" />
            </div>
            <div className="flex flex-col">
              <h2 className="text-xl font-semibold text-foreground">
                BoxFlux
              </h2>
              <p className="text-sm text-muted-foreground">
                Optimización y conversión avanzada de archivos
              </p>
              <p className="mt-0.5 font-mono text-2xs text-muted-foreground/80">
                v0.4.1
              </p>
            </div>
          </div>

          <Divider />

          {/* Stats grid */}
          <section>
            <span className="caption">Versión</span>
            <dl className="mt-2 grid grid-cols-1 gap-3 sm:grid-cols-2">
              <Stat
                icon={Cpu}
                label="Engine"
                value={info ? `v${info.engine_version}` : "—"}
              />
              <Stat
                icon={Brain}
                label="Intelligence"
                value={info ? `v${info.intel_engine_version}` : "—"}
              />
              <Stat
                icon={Cpu}
                label="Workers"
                value={info ? `${info.max_workers} threads` : "—"}
              />
              <Stat
                icon={Shield}
                label="Licencia"
                value={`${tier} — ${status}`}
              />
            </dl>
          </section>

          <Divider />

          {/* Backends */}
          <section>
            <span className="caption">Backends cargados</span>
            <div className="mt-2 flex flex-wrap gap-1.5">
              {backends.length > 0 ? (
                backends.map(([id, name]) => (
                  <span
                    key={id}
                    className="inline-flex items-center gap-1.5 rounded-xs border border-border bg-surface-inset px-2 py-1 font-mono text-xs text-foreground"
                  >
                    <span className="inline-block h-1.5 w-1.5 rounded-full bg-success" />
                    {name}
                  </span>
                ))
              ) : (
                <span className="text-sm text-muted-foreground">
                  App de escritorio requerida. Ejecuta{" "}
                  <code className="font-mono text-xs">pnpm tauri:dev</code>{" "}
                  para cargar los backends de optimización.
                </span>
              )}
            </div>
          </section>

          <Divider />

          <footer className="text-xs leading-relaxed text-muted-foreground">
            <p className="font-medium text-foreground">© 2026 Box Studio</p>
            <p className="mt-1">
              Aplicación de escritorio construida con Rust + Tauri 2 + React +
              TypeScript. Sin Qt, sin C++. Pensada para fiabilidad de ingeniería.
            </p>
          </footer>
        </CardContent>
      </Card>
    </div>
  );
};

const Stat: React.FC<{
  icon: React.ElementType;
  label: string;
  value: string;
}> = ({ icon: Icon, label, value }) => (
  <div className="flex items-center gap-2.5 rounded-md border border-border bg-surface-inset px-3 py-2">
    <Icon className="h-4 w-4 shrink-0 text-muted-foreground" aria-hidden="true" />
    <div className="flex flex-col min-w-0 flex-1">
      <span className="text-2xs font-medium text-muted-foreground">
        {label}
      </span>
      <span className="truncate font-mono text-sm font-medium text-foreground">
        {value}
      </span>
    </div>
  </div>
);

AboutPage.displayName = "AboutPage";
