import { useEffect, useMemo, useRef, useState } from "react";
import { Download, Eraser, Search, X, Minus, CircleDot } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { runtimeCommands, type MinecraftLogChunk } from "../lib/runtimeCommands";

type LogLevel = "all" | "info" | "warn" | "error" | "debug" | "snine";

type LogLine = {
  id: number;
  text: string;
  level: Exclude<LogLevel, "all"> | "plain";
};

function classifyLine(text: string): LogLine["level"] {
  const value = text.toLowerCase();
  if (value.includes("[snine") || value.includes("snine client") || value.includes("snine launcher")) return "snine";
  if (value.includes("fatal") || value.includes("error") || value.includes("exception") || value.includes("crash")) return "error";
  if (value.includes("warn") || value.includes("warning")) return "warn";
  if (value.includes("debug") || value.includes("trace")) return "debug";
  if (value.includes("info") || value.includes("[main/") || value.includes("[render thread/")) return "info";
  return "plain";
}

function parseParams() {
  const params = new URLSearchParams(window.location.search);
  return {
    profileId: params.get("profileId") ?? "",
    launchId: params.get("launchId") ?? "",
    accountName: params.get("accountName") ?? "Minecraft",
  };
}

function windowCommand(name: string) {
  if ("__TAURI_INTERNALS__" in window) void invoke(name);
}

export function MinecraftLogWindow() {
  const params = useMemo(parseParams, []);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [offset, setOffset] = useState(0);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState<LogLevel>("all");
  const [autoScroll, setAutoScroll] = useState(true);
  const [status, setStatus] = useState<"starting" | "running" | "stopping" | "exited" | "failed">("starting");
  const [exportStatus, setExportStatus] = useState("");
  const nextId = useRef(0);
  const viewport = useRef<HTMLDivElement | null>(null);
  const offsetRef = useRef(0);

  useEffect(() => { offsetRef.current = offset; }, [offset]);

  useEffect(() => {
    if (!params.profileId || !params.launchId) return;
    let disposed = false;
    let busy = false;

    const poll = async () => {
      if (disposed || busy) return;
      busy = true;
      try {
        const [chunk, launches] = await Promise.all([
          runtimeCommands.logRead(params.profileId, params.launchId, offsetRef.current),
          runtimeCommands.launchStatuses(),
        ]);
        if (disposed) return;
        if (chunk.text) {
          const parsed = chunk.text
            .replace(/\r\n?/g, "\n")
            .split("\n")
            .filter((text, index, list) => text.length > 0 || index < list.length - 1)
            .map((text) => ({ id: nextId.current++, text, level: classifyLine(text) }));
          setLines((current) => {
            const next = [...current, ...parsed];
            return next.length > 12000 ? next.slice(next.length - 12000) : next;
          });
          offsetRef.current = chunk.nextOffset;
          setOffset(chunk.nextOffset);
        }
        const launch = launches.find((item) => item.launchId === params.launchId);
        if (launch) {
          const state = String(launch.state);
          setStatus(state === "running" ? "running" : state === "stopping" ? "stopping" : state === "failed" ? "failed" : state === "exited" ? "exited" : "starting");
        }
      } catch (error) {
        if (!disposed) console.warn("[SNine Logs] polling failed", error);
      } finally {
        busy = false;
      }
    };

    void poll();
    const timer = window.setInterval(() => void poll(), 450);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [params.launchId, params.profileId]);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return lines.filter((line) => {
      if (level !== "all" && line.level !== level) return false;
      return !needle || line.text.toLowerCase().includes(needle);
    });
  }, [level, lines, query]);

  useEffect(() => {
    if (!autoScroll) return;
    const element = viewport.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [autoScroll, visible.length]);

  const exportLogs = async () => {
    setExportStatus("");
    try {
      const path = await runtimeCommands.logExport(params.profileId, params.launchId);
      setExportStatus(`Exportiert: ${path}`);
    } catch (error) {
      setExportStatus(`Export fehlgeschlagen: ${String(error)}`);
    }
  };

  return (
    <div className="snine-log-window">
      <header className="snine-log-window__titlebar" data-tauri-drag-region>
        <div data-tauri-drag-region>
          <span className="snine-log-window__mark">⚡</span>
          <strong>SNINE · MINECRAFT LOGS</strong>
          <span className={`snine-log-window__status is-${status}`}><CircleDot aria-hidden="true" /> {status.toUpperCase()}</span>
        </div>
        <div>
          <button type="button" onClick={() => windowCommand("window_minimize")} aria-label="Minimieren"><Minus /></button>
          <button type="button" onClick={() => windowCommand("window_close")} aria-label="Schließen"><X /></button>
        </div>
      </header>

      <section className="snine-log-window__toolbar">
        <label className="snine-log-window__search">
          <Search aria-hidden="true" />
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Logs durchsuchen..." />
          {query ? <button type="button" onClick={() => setQuery("")}><X /></button> : null}
        </label>
        <div className="snine-log-window__filters">
          {(["all", "snine", "info", "warn", "error", "debug"] as LogLevel[]).map((value) => (
            <button key={value} type="button" className={level === value ? "is-active" : ""} onClick={() => setLevel(value)}>{value.toUpperCase()}</button>
          ))}
        </div>
        <label className="snine-log-window__autoscroll">
          <input type="checkbox" checked={autoScroll} onChange={(event) => setAutoScroll(event.target.checked)} />
          AUTO-SCROLL
        </label>
        <button type="button" className="snine-log-window__action" onClick={() => { nextId.current = 0; setLines([]); }} title="Ansicht leeren"><Eraser /></button>
        <button type="button" className="snine-log-window__action is-primary" onClick={() => void exportLogs()}><Download /> EXPORT</button>
      </section>

      <div className="snine-log-window__meta">
        <span>PLAYER <strong>{params.accountName}</strong></span>
        <span>LAUNCH <strong>{params.launchId}</strong></span>
        <span>ZEILEN <strong>{visible.length.toLocaleString("de-DE")}</strong></span>
        {exportStatus ? <span className="snine-log-window__export-status">{exportStatus}</span> : null}
      </div>

      <main ref={viewport} className="snine-log-window__viewport" onScroll={(event) => {
        const element = event.currentTarget;
        const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 32;
        if (!atBottom && autoScroll) setAutoScroll(false);
      }}>
        {visible.length ? visible.map((line) => (
          <div key={line.id} className={`snine-log-line is-${line.level}`}>
            <span>{String(line.id + 1).padStart(5, "0")}</span>
            <code>{line.text || " "}</code>
          </div>
        )) : (
          <div className="snine-log-window__empty">Warte auf Minecraft-Ausgabe …</div>
        )}
      </main>
    </div>
  );
}
