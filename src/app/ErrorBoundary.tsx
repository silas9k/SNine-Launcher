import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { Button } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";

interface State { failed: boolean }

class Boundary extends Component<{ children: ReactNode; fallback: ReactNode }, State> {
  state: State = { failed: false };
  static getDerivedStateFromError(): State { return { failed: true }; }
  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("SNine UI boundary", error.name, info.componentStack);
  }
  render(): ReactNode { return this.state.failed ? this.props.fallback : this.props.children; }
}

function FatalFallback() {
  const { t } = useI18n();
  return (
    <main className="fatal-state" id="main-content" tabIndex={0}>
      <AlertTriangle aria-hidden="true" />
      <h1>{t("app.unavailableTitle")}</h1>
      <Button onClick={() => window.location.reload()}>{t("app.retry")}</Button>
    </main>
  );
}

export function AppErrorBoundary({ children }: { children: ReactNode }) {
  return <Boundary fallback={<FatalFallback />}>{children}</Boundary>;
}
