import { useEffect, useRef } from "react";
import type { Artifact } from "@a2a-js/sdk";
import ArtifactPreview from "./ArtifactPreview";

export type ConsoleEntryType = "user" | "agent" | "status" | "artifact" | "task" | "negotiation";

export interface ConsoleEntry {
  id: string;
  type: ConsoleEntryType;
  title: string;
  body?: string;
  subtext?: string;
  taskId?: string;
  contextId?: string;
  timestamp: string;
  badge?: string;
  artifact?: Artifact;
}

interface ConsoleProps {
  entries: ConsoleEntry[];
  isStreaming: boolean;
  title?: string;
  className?: string;
}

const TYPE_STYLES: Record<
  ConsoleEntryType,
  { dot: string; border: string; badge: string; title: string }
> = {
  user: {
    dot: "bg-sky-400",
    border: "border-sky-500/30",
    badge: "bg-sky-500/10 text-sky-300",
    title: "text-sky-200",
  },
  agent: {
    dot: "bg-emerald-400",
    border: "border-emerald-500/30",
    badge: "bg-emerald-500/10 text-emerald-200",
    title: "text-emerald-200",
  },
  negotiation: {
    dot: "bg-cyan-400",
    border: "border-cyan-500/30",
    badge: "bg-cyan-500/10 text-cyan-200",
    title: "text-cyan-200",
  },
  status: {
    dot: "bg-amber-400",
    border: "border-amber-500/30",
    badge: "bg-amber-500/10 text-amber-200",
    title: "text-amber-200",
  },
  artifact: {
    dot: "bg-violet-400",
    border: "border-violet-500/30",
    badge: "bg-violet-500/10 text-violet-200",
    title: "text-violet-200",
  },
  task: {
    dot: "bg-blue-400",
    border: "border-blue-500/30",
    badge: "bg-blue-500/10 text-blue-200",
    title: "text-blue-200",
  },
};

export default function Console({
  entries,
  isStreaming,
  title = "Console",
  className = "h-[30rem]",
}: ConsoleProps) {
  const eventsEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    eventsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [entries]);

  const containerClasses = [
    "bg-slate-950 border border-slate-800 rounded-2xl shadow-2xl text-slate-100 flex flex-col",
    className,
  ].join(" ");

  return (
    <div className={containerClasses}>
      <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
        <div>
          <p className="text-sm font-semibold text-white/80">{title}</p>
          <p className="text-xs text-slate-400">
            Stream live task messages and status updates.
          </p>
        </div>
        <div className="flex items-center gap-2 text-xs font-semibold tracking-wide uppercase">
          <span
            className={`inline-flex h-2.5 w-2.5 rounded-full ${
              isStreaming ? "bg-amber-400 animate-pulse" : "bg-emerald-400"
            }`}
          />
          <span className="text-slate-400">
            {isStreaming ? "Streaming" : "Idle"}
          </span>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        {entries.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center text-center text-slate-500">
            <p className="text-sm">No activity yet.</p>
            <p className="text-xs text-slate-600">
              Send a message to start the conversation.
            </p>
          </div>
        ) : (
          entries.map(entry => {
            const styles = TYPE_STYLES[entry.type];
            return (
              <div key={entry.id} className="flex gap-3">
                <span
                  className={`mt-2 h-2 w-2 rounded-full ${styles.dot}`}
                  aria-hidden="true"
                />
                <div className={`flex-1 border-l pl-4 ${styles.border}`}>
                  <div className="flex flex-wrap items-center gap-2">
                    <p className={`text-sm font-semibold ${styles.title}`}>
                      {entry.title}
                    </p>
                    {entry.badge && (
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-semibold ${styles.badge}`}
                      >
                        {entry.badge}
                      </span>
                    )}
                    <span className="text-[11px] uppercase tracking-wide text-slate-500">
                      {new Date(entry.timestamp).toLocaleTimeString()}
                    </span>
                  </div>

                  {entry.body && (
                    <p className="mt-2 whitespace-pre-wrap text-sm leading-relaxed text-slate-200">
                      {entry.body}
                    </p>
                  )}
                  {entry.subtext && (
                    <p className="mt-1 text-xs text-slate-500">{entry.subtext}</p>
                  )}
                  {entry.artifact && (
                    <div className="mt-3">
                      <ArtifactPreview artifact={entry.artifact} />
                    </div>
                  )}
                </div>
              </div>
            );
          })
        )}
        <div ref={eventsEndRef} />
      </div>
    </div>
  );
}
