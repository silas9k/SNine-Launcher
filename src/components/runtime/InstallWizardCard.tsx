import { useReleaseText } from "../../i18n/releaseUiText";
import { Check, Download, LoaderCircle, Wrench, CircleX } from "lucide-react";

export type InstallStep = {
  id: string;
  label: string;
  state: "done" | "running" | "missing" | "error";
};

export function InstallWizardCard({ steps }: { steps: InstallStep[] }) {
  const rt = useReleaseText();
  return (
    <section className="snine-install-wizard snine-install-wizard--card">
      <header>
        <Wrench size={18} />
        <strong>{rt("SNine Setup")}</strong>
      </header>

      <div className="snine-install-wizard__steps">
        {steps.map((step) => (
          <div key={step.id} className={`snine-install-wizard__step ${step.state}`}>
            {step.state === "done" ? (
              <Check size={16} />
            ) : step.state === "running" ? (
              <LoaderCircle className="ui-spin" size={16} />
            ) : step.state === "error" ? (
              <CircleX size={16} />
            ) : (
              <Download size={16} />
            )}
            <span>{step.label}</span>
            <small>
              {step.state === "done"
                ? "READY"
                : step.state === "running"
                  ? "CHECKING"
                  : step.state === "error"
                    ? "CHECK FAILED"
                    : "MISSING"}
            </small>
          </div>
        ))}
      </div>
    </section>
  );
}
