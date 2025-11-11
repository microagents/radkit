import type { StateTransition } from "../types/StateTransition";

interface StateTransitionTimelineProps {
  transitions: StateTransition[];
}

const STATE_COLORS: Record<string, { bg: string; text: string; border: string }> = {
  Submitted: { bg: "bg-blue-500/10", text: "text-blue-300", border: "border-blue-500/30" },
  Working: { bg: "bg-amber-500/10", text: "text-amber-300", border: "border-amber-500/30" },
  InputRequired: { bg: "bg-purple-500/10", text: "text-purple-300", border: "border-purple-500/30" },
  Completed: { bg: "bg-emerald-500/10", text: "text-emerald-300", border: "border-emerald-500/30" },
  Failed: { bg: "bg-red-500/10", text: "text-red-300", border: "border-red-500/30" },
  Canceled: { bg: "bg-slate-500/10", text: "text-slate-300", border: "border-slate-500/30" },
  Rejected: { bg: "bg-orange-500/10", text: "text-orange-300", border: "border-orange-500/30" },
};

const TRIGGER_LABELS: Record<string, string> = {
  on_request: "User Request",
  on_input_received: "Input Received",
  status_update: "Status Update",
};

const getStateColor = (state: string) => {
  return STATE_COLORS[state] || { bg: "bg-slate-500/10", text: "text-slate-300", border: "border-slate-500/30" };
};

export default function StateTransitionTimeline({ transitions }: StateTransitionTimelineProps) {
  if (transitions.length === 0) {
    return (
      <div className="rounded-xl bg-slate-900/50 p-4 border border-slate-800">
        <p className="text-xs text-slate-500">No state transitions recorded yet.</p>
      </div>
    );
  }

  return (
    <div className="space-y-3 rounded-2xl bg-slate-900/50 p-4 border border-slate-800">
      <p className="text-xs font-semibold uppercase tracking-wide text-slate-400">
        State Transitions
      </p>

      <div className="space-y-3">
        {transitions.map((transition, index) => {
          const toColor = getStateColor(transition.to_state);
          const fromColor = transition.from_state ? getStateColor(transition.from_state) : null;

          return (
            <div key={index} className="flex gap-3">
              {/* Timeline indicator */}
              <div className="flex flex-col items-center">
                <div className={`h-3 w-3 rounded-full ${toColor.bg} border-2 ${toColor.border}`} />
                {index < transitions.length - 1 && (
                  <div className="w-0.5 flex-1 bg-slate-700 min-h-6" />
                )}
              </div>

              {/* Transition content */}
              <div className="flex-1 pb-2">
                <div className="flex items-center gap-2 flex-wrap">
                  {/* From state (if present) */}
                  {transition.from_state && (
                    <>
                      <span className={`rounded-full border px-2 py-0.5 text-xs font-semibold ${fromColor?.border} ${fromColor?.bg} ${fromColor?.text}`}>
                        {transition.from_state}
                      </span>
                      <span className="text-slate-500 text-xs">→</span>
                    </>
                  )}

                  {/* To state */}
                  <span className={`rounded-full border px-2 py-0.5 text-xs font-semibold ${toColor.border} ${toColor.bg} ${toColor.text}`}>
                    {transition.to_state}
                  </span>

                  {/* Trigger badge */}
                  <span className="text-[10px] uppercase tracking-wide text-slate-500 bg-slate-800/50 px-2 py-0.5 rounded-full">
                    {TRIGGER_LABELS[transition.trigger] || transition.trigger}
                  </span>
                </div>

                {/* Timestamp */}
                <p className="text-[11px] text-slate-500 mt-1">
                  {new Date(transition.timestamp).toLocaleString()}
                </p>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
