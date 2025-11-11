import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  Message,
  Task as A2ATask,
  TaskStatusUpdateEvent,
  TaskArtifactUpdateEvent,
} from "@a2a-js/sdk";
import { Link, useLoaderData } from "react-router";
import type { LoaderFunctionArgs } from "react-router";
import {
  getAgentDetail,
  getTaskHistory,
  listContexts,
  listContextTasks,
  sendMessageStream,
  resubscribeTaskStream,
  type A2AStreamEvent,
  type TaskSummary,
} from "../api/client";
import ArtifactPreview from "../components/ArtifactPreview";

export async function loader({ params }: LoaderFunctionArgs) {
  const agentId = params.agentId!;
  const detail = await getAgentDetail(agentId);
  return { detail };
}

function shortenId(id: string): string {
  return `${id.slice(0, 6)}...${id.slice(-4)}`;
}

// Console entry types
type ConsoleEntryType = "user" | "agent" | "status" | "artifact" | "task" | "negotiation";

interface ConsoleEntry {
  id: string;
  type: ConsoleEntryType;
  title: string;
  body?: string;
  subtext?: string;
  taskId?: string;
  contextId?: string;
  timestamp: string;
  badge?: string;
  artifact?: import("@a2a-js/sdk").Artifact;
}

export default function AgentDetail() {
  const { detail } = useLoaderData<Awaited<ReturnType<typeof loader>>>();

  // Context and messaging state
  const [availableContexts, setAvailableContexts] = useState<string[]>([]);
  const [contextId, setContextId] = useState<string | null>(null);
  const [timeline, setTimeline] = useState<ConsoleEntry[]>([]);
  const [inputValue, setInputValue] = useState("");
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Task management state
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [inspectedTask, setInspectedTask] = useState<{
    taskId: string;
    entries: ConsoleEntry[];
    task?: A2ATask;
  } | null>(null);
  const [inspecting, setInspecting] = useState(false);

  // Active interaction tracking
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null); // Task user is replying to
  const [isInNegotiation, setIsInNegotiation] = useState(false); // Negotiation phase (no task yet)
  const streamCancelRef = useRef<(() => void) | null>(null);
  const contextIdRef = useRef<string | null>(null);
  const [showAgentInfo, setShowAgentInfo] = useState(false);
  const consoleContainerRef = useRef<HTMLDivElement | null>(null);

  const refreshContexts = useCallback(async () => {
    try {
      const contexts = await listContexts(detail.id, detail.version);
      setAvailableContexts(contexts);
    } catch (err) {
      console.error(err);
    }
  }, [detail.id, detail.version]);

  const ensureContextListed = useCallback((ctxId: string) => {
    if (!ctxId) return;
    setAvailableContexts(prev =>
      prev.includes(ctxId) ? prev : [ctxId, ...prev]
    );
  }, []);

  const upsertTaskFromEvent = useCallback(
    (incoming: A2ATask) => {
      setTasks(prev => {
        const next: TaskSummary[] = [];
        let found = false;
        for (const summary of prev) {
          if (summary.task.id === incoming.id) {
            next.push({
              ...summary,
              task: { ...incoming },
            });
            found = true;
          } else {
            next.push(summary);
          }
        }
        if (!found) {
          next.unshift({ task: incoming });
        }
        return next;
      });
    },
    []
  );

  const applyStatusUpdate = useCallback((event: TaskStatusUpdateEvent) => {
    setTasks(prev => {
      let found = false;
      const updated = prev.map(summary => {
        if (summary.task.id === event.taskId) {
          found = true;
          return {
            ...summary,
            task: {
              ...summary.task,
              status: event.status,
            },
          };
        }
        return summary;
      });

      if (!found) {
        const placeholder: A2ATask = {
          id: event.taskId,
          contextId: event.contextId,
          status: event.status,
          history: [],
          artifacts: [],
          kind: "task",
          metadata: undefined,
        };
        return [{ task: placeholder }, ...updated];
      }

      return updated;
    });
  }, []);

  const cancelActiveStream = useCallback(() => {
    if (streamCancelRef.current) {
      streamCancelRef.current();
      streamCancelRef.current = null;
    }
  }, []);

  const pushEntry = useCallback((entry: ConsoleEntry) => {
    setTimeline(prev => {
      if (prev.some(existing => existing.id === entry.id)) {
        return prev;
      }
      return [...prev, entry];
    });
  }, []);

  const processStreamEvent = useCallback(
    (event: A2AStreamEvent) => {
      const ctxId = "contextId" in event ? event.contextId : undefined;
      if (ctxId) {
        ensureContextListed(ctxId);
        setContextId(prev => prev ?? ctxId);
      }

      if (isTaskEvent(event)) {
        upsertTaskFromEvent(event);
        const needsInput = event.status.state === "input-required";
        setIsInNegotiation(!needsInput);
        setActiveTaskId(needsInput ? event.id : null);
      } else if (isStatusUpdateEvent(event)) {
        applyStatusUpdate(event);
        setIsInNegotiation(false);
        if (event.status.state === "input-required") {
          setIsInNegotiation(false);
          setActiveTaskId(event.taskId);
        } else {
          setActiveTaskId(prev => (prev === event.taskId ? null : prev));
        }
        if (event.final) {
          setIsInNegotiation(false);
        }
      } else if (isMessageEvent(event)) {
        if (!event.taskId) {
          // pure negotiation message
          setIsInNegotiation(true);
          setActiveTaskId(null);
        }
      }

      const entry = convertEventToEntry(event);
      if (entry) {
        pushEntry(entry);
      }
    },
    [applyStatusUpdate, ensureContextListed, pushEntry, upsertTaskFromEvent]
  );

  // Load available contexts on mount
  useEffect(() => {
    refreshContexts();
  }, [refreshContexts]);

  useEffect(() => {
    contextIdRef.current = contextId;
  }, [contextId]);

  useEffect(() => () => cancelActiveStream(), [cancelActiveStream]);

  // Load tasks when context changes
  useEffect(() => {
    if (contextId) {
      listContextTasks(detail.id, detail.version, contextId)
        .then(setTasks)
        .catch(console.error);
    } else {
      setTasks([]);
    }
  }, [contextId, detail.id, detail.version]);

  // Determine what user is replying to
  const activeInteraction = useMemo(() => {
    if (activeTaskId) {
      const task = tasks.find(t => t.task.id === activeTaskId);
      // Check if task still exists and is in valid state for input
      const canReceiveInput = task && task.task.status.state === "input-required";
      return {
        type: "task" as const,
        taskId: activeTaskId,
        task,
        canReceiveInput
      };
    }
    if (isInNegotiation) {
      return { type: "negotiation" as const };
    }
    if (contextId) {
      return { type: "context" as const };
    }
    return { type: "new" as const };
  }, [activeTaskId, isInNegotiation, contextId, tasks]);

  // Clear active task if it's no longer in input-required state
  useEffect(() => {
    if (activeInteraction.type === "task" && !activeInteraction.canReceiveInput) {
      setActiveTaskId(null);
    }
  }, [activeInteraction]);

  const handleSendMessage = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim() || isStreaming) return;

    // Validate if sending to a task
    if (activeTaskId) {
      const task = tasks.find(t => t.task.id === activeTaskId);
      if (!task || task.task.status.state !== "input-required") {
        setError("Cannot send message to this task - it is not waiting for input");
        return;
      }
    }

    setIsStreaming(true);
    setError(null);
    cancelActiveStream();

    const userMessage = inputValue;
    setInputValue("");

    // Add user message to timeline
    setTimeline(prev => [
      ...prev,
      {
        id: `user-${Date.now()}`,
        type: "user",
        title: "You",
        body: userMessage,
        timestamp: new Date().toISOString(),
      },
    ]);

    try {
      const stream = sendMessageStream(detail.id, {
        messageText: userMessage,
        contextId: contextId ?? undefined,
        taskId: activeTaskId ?? undefined,
      });
      const iterator = stream[Symbol.asyncIterator]();
      streamCancelRef.current = () => {
        iterator.return?.();
      };
      while (true) {
        const { value, done } = await iterator.next();
        if (done) {
          break;
        }
        if (value) {
          processStreamEvent(value);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to send message");
    } finally {
      cancelActiveStream();
      setIsStreaming(false);
      // Reload tasks after message
      const contextToRefresh = contextIdRef.current;
      if (contextToRefresh) {
        listContextTasks(detail.id, detail.version, contextToRefresh)
          .then(setTasks)
          .catch(console.error);
      }
      refreshContexts();
    }
  };

  const handleInspectTask = useCallback(
    async (taskId: string) => {
      setInspecting(true);
      try {
        const history = await getTaskHistory(detail.id, detail.version, taskId);
      const entries = history.events
        .map(({ result }) => convertEventToEntry(result))
        .filter(
          (entry): entry is ConsoleEntry =>
            Boolean(entry)
        ) as ConsoleEntry[];
      setInspectedTask({
          taskId,
          entries,
          task: history.task,
        });
      } catch (err) {
        console.error(err);
        setError(err instanceof Error ? err.message : "Failed to load task");
      } finally {
        setInspecting(false);
      }
    },
    [detail.id, detail.version]
  );

  const handleNewContext = () => {
    setContextId(null);
    setTimeline([]);
    setTasks([]);
    setInspectedTask(null);
    setActiveTaskId(null);
    setIsInNegotiation(false);
    refreshContexts();
  };

  const handleSelectContext = (ctxId: string) => {
    cancelActiveStream();
    setContextId(ctxId);
    setTimeline([]);
    setInspectedTask(null);
    setActiveTaskId(null);
    setIsInNegotiation(false);
  };

  const loadTaskHistoryIntoTimeline = useCallback(
    async (taskId: string) => {
      const history = await getTaskHistory(detail.id, detail.version, taskId);
      const entries = history.events
        .map(({ result }) => convertEventToEntry(result))
        .filter(Boolean) as ConsoleEntry[];
      const unique = Array.from(
        entries.reduce((acc, entry) => acc.set(entry.id, entry), new Map<string, ConsoleEntry>()).values()
      );
      setTimeline(unique);
      if (history.task?.contextId) {
        ensureContextListed(history.task.contextId);
        setContextId(history.task.contextId);
      }
      return history;
    },
    [detail.id, detail.version, ensureContextListed]
  );

  const handleSubscribeTask = useCallback(
    async (taskId: string) => {
      let contextForRefresh: string | null = null;
      try {
        setIsStreaming(true);
        cancelActiveStream();
        const history = await loadTaskHistoryIntoTimeline(taskId);
        if (history.task) {
          upsertTaskFromEvent(history.task);
          setIsInNegotiation(false);
          if (history.task.status.state === "input-required") {
            setActiveTaskId(taskId);
          }
          contextForRefresh = history.task.contextId;
        }

        const stream = resubscribeTaskStream(detail.id, { id: taskId });
        const iterator = stream[Symbol.asyncIterator]();
        streamCancelRef.current = () => {
          iterator.return?.();
        };

        while (true) {
          const { value, done } = await iterator.next();
          if (done) {
            break;
          }
          if (value) {
            processStreamEvent(value);
          }
        }
      } catch (err) {
        console.error("Failed to subscribe to task:", err);
        setError(err instanceof Error ? err.message : "Failed to subscribe to task");
      } finally {
        cancelActiveStream();
        setIsStreaming(false);
        refreshContexts();
        const targetContext = contextForRefresh ?? contextIdRef.current;
        if (targetContext) {
          listContextTasks(detail.id, detail.version, targetContext)
            .then(setTasks)
            .catch(console.error);
        }
      }
    },
    [
      cancelActiveStream,
      detail.id,
      detail.version,
      loadTaskHistoryIntoTimeline,
      processStreamEvent,
      refreshContexts,
      upsertTaskFromEvent,
    ]
  );

  const handleStartNewInContext = () => {
    cancelActiveStream();
    setActiveTaskId(null);
    setIsInNegotiation(false);
    setTimeline([]);
  };

  const handleResumeTask = useCallback(
    async (taskId: string) => {
      try {
        cancelActiveStream();
        const history = await loadTaskHistoryIntoTimeline(taskId);
        if (history.task) {
          const needsInput = history.task.status.state === "input-required";
          setActiveTaskId(needsInput ? taskId : null);
          setIsInNegotiation(!needsInput);
          upsertTaskFromEvent(history.task);
        }
      } catch (err) {
        console.error("Failed to resume task:", err);
        setError(err instanceof Error ? err.message : "Failed to resume task");
      }
    },
    [cancelActiveStream, loadTaskHistoryIntoTimeline, upsertTaskFromEvent]
  );

  useEffect(() => {
    const container = consoleContainerRef.current;
    if (!container) {
      return;
    }
    container.scrollTo({
      top: container.scrollHeight,
      behavior: "smooth",
    });
  }, [timeline, isStreaming]);

  return (
    <div className="flex flex-1 h-full min-h-0 w-full flex-col overflow-hidden bg-slate-50 dark:bg-zinc-900 rounded-2xl border border-slate-200 dark:border-zinc-800">
      {/* Header */}
      <header className="border-b border-slate-200 bg-white px-6 py-4 shadow-sm dark:border-zinc-800 dark:bg-zinc-900">
        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-3">
              <Link
                to="/"
                className="text-slate-400 hover:text-slate-600 transition-colors dark:text-zinc-500 dark:hover:text-zinc-300"
              >
                ←
              </Link>
              <h1 className="text-2xl font-bold text-slate-900 dark:text-zinc-100">{detail.name}</h1>
              <span className="rounded-full bg-slate-100 px-3 py-1 text-xs font-semibold text-slate-600 dark:bg-zinc-800 dark:text-zinc-200">
                v{detail.version}
              </span>
              <button
                onClick={() => setShowAgentInfo(true)}
                className="rounded-full border border-blue-200 bg-blue-50 px-3 py-1 text-xs font-semibold text-blue-700 hover:bg-blue-100 transition-colors dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-100 dark:hover:bg-zinc-700"
                type="button"
              >
                Info
              </button>
            </div>
            <p className="mt-1 text-sm text-slate-600 dark:text-zinc-300">{detail.description}</p>
          </div>
        </div>
      </header>

      <div className="flex flex-1 overflow-hidden min-h-0">
        {/* Main chat area */}
        <div className="flex flex-1 flex-col min-h-0">
          {/* Console (scrollable) */}
          <div
            ref={consoleContainerRef}
            className="flex-1 overflow-y-auto bg-white px-6 py-6 dark:bg-zinc-950"
          >
            {timeline.length === 0 ? (
              <div className="flex h-full items-center justify-center text-center">
                <div>
                  <p className="text-sm text-slate-500 dark:text-zinc-400">No messages yet</p>
                  <p className="text-xs text-slate-400 mt-1 dark:text-zinc-500">
                    {contextId ? "Start a conversation" : "Select or create a context to begin"}
                  </p>
                </div>
              </div>
            ) : (
              <div className="space-y-6 max-w-4xl mx-auto">
                {timeline.map((entry) => {
                  const isUser = entry.type === "user";
                  const isNegotiation = entry.type === "negotiation";
                  const isTask = entry.type === "task";

                  return (
                    <div key={entry.id} className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
                      <div className={`max-w-[80%] ${isUser ? "" : "w-full"}`}>
                        {/* Message header */}
                        {!isUser && (
                          <div className="flex items-center gap-2 mb-2">
                            <span className="text-xs font-semibold text-slate-700 dark:text-zinc-200">
                              {entry.title}
                            </span>
                            {entry.badge && (
                              <span className="text-[10px] uppercase tracking-wide px-2 py-0.5 rounded-full bg-slate-100 text-slate-600 font-semibold dark:bg-zinc-800 dark:text-zinc-300">
                                {entry.badge}
                              </span>
                            )}
                            <span className="text-[11px] text-slate-400 dark:text-zinc-500">
                              {new Date(entry.timestamp).toLocaleTimeString()}
                            </span>
                          </div>
                        )}

                        {/* Message content */}
                        {isUser ? (
                          <div className="rounded-2xl bg-blue-600 px-4 py-3 text-white dark:bg-blue-500">
                            <p className="text-sm whitespace-pre-wrap">{entry.body}</p>
                          </div>
                        ) : isTask ? (
                          <div className="rounded-2xl border-2 border-blue-200 bg-blue-50 px-4 py-3 dark:border-blue-500/40 dark:bg-blue-500/10">
                            <p className="text-sm font-semibold text-blue-900 mb-1 dark:text-blue-200">{entry.title}</p>
                            {entry.body && <p className="text-sm text-blue-800 whitespace-pre-wrap dark:text-blue-100">{entry.body}</p>}
                            {entry.subtext && <p className="text-xs text-blue-600 mt-2 dark:text-blue-200">{entry.subtext}</p>}
                            {entry.artifact && (
                              <div className="mt-3">
                                <ArtifactPreview artifact={entry.artifact} />
                              </div>
                            )}
                          </div>
                        ) : isNegotiation ? (
                          <div className="rounded-2xl border border-cyan-200 bg-cyan-50 px-4 py-3 dark:border-cyan-500/40 dark:bg-cyan-500/10">
                            {entry.body && <p className="text-sm text-cyan-900 whitespace-pre-wrap dark:text-cyan-100">{entry.body}</p>}
                          </div>
                        ) : (
                          <div className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 dark:border-zinc-700 dark:bg-zinc-800">
                            {entry.body && <p className="text-sm text-slate-700 whitespace-pre-wrap dark:text-zinc-200">{entry.body}</p>}
                            {entry.subtext && <p className="text-xs text-slate-500 mt-2 dark:text-zinc-400">{entry.subtext}</p>}
                            {entry.artifact && (
                              <div className="mt-3">
                                <ArtifactPreview artifact={entry.artifact} />
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
                {isStreaming && (
                  <div className="flex justify-start">
                    <div className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 dark:border-zinc-700 dark:bg-zinc-800">
                      <div className="flex gap-1">
                        <div className="h-2 w-2 rounded-full bg-slate-400 animate-bounce dark:bg-zinc-700" />
                        <div className="h-2 w-2 rounded-full bg-slate-400 animate-bounce [animation-delay:0.2s] dark:bg-zinc-700" />
                        <div className="h-2 w-2 rounded-full bg-slate-400 animate-bounce [animation-delay:0.4s] dark:bg-zinc-700" />
                      </div>
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Message input (fixed at bottom) */}
          <div className="border-t border-slate-200 bg-white px-6 py-4 flex-shrink-0 dark:border-zinc-800 dark:bg-zinc-900">
            {error && (
              <div className="mb-3 rounded-lg bg-red-50 px-4 py-2 text-sm text-red-600 dark:bg-red-500/20 dark:text-red-200">
                {error}
              </div>
            )}

            {/* Active interaction indicator */}
            <div className="mb-3">
              {activeInteraction.type === "task" && activeInteraction.task && (
                <>
                  {activeInteraction.canReceiveInput ? (
                    <div className="flex items-center justify-between rounded-lg border border-purple-200 bg-purple-50 px-4 py-2 dark:border-purple-500/40 dark:bg-purple-500/10">
                      <div className="flex items-center gap-2">
                        <div className="h-2 w-2 rounded-full bg-purple-500 dark:bg-purple-300" />
                        <span className="text-xs font-semibold text-purple-900 dark:text-purple-100">
                          Replying to Task {shortenId(activeInteraction.taskId)}
                        </span>
                        <span className="text-[10px] uppercase tracking-wide px-2 py-0.5 rounded-full bg-purple-100 text-purple-700 font-semibold dark:bg-purple-400/20 dark:text-purple-100">
                          {activeInteraction.task.task.status.state}
                        </span>
                      </div>
                      <button
                        onClick={handleStartNewInContext}
                        className="text-xs text-purple-700 hover:text-purple-900 font-medium dark:text-purple-200 dark:hover:text-purple-100"
                      >
                        Start new conversation
                      </button>
                    </div>
                  ) : (
                    <div className="flex items-center justify-between rounded-lg border border-red-200 bg-red-50 px-4 py-2 dark:border-red-400/30 dark:bg-red-500/10">
                      <div className="flex items-center gap-2">
                        <div className="h-2 w-2 rounded-full bg-red-500 dark:bg-red-300" />
                        <span className="text-xs font-semibold text-red-900 dark:text-red-100">
                          Task {shortenId(activeInteraction.taskId)} is not waiting for input
                        </span>
                        <span className="text-[10px] uppercase tracking-wide px-2 py-0.5 rounded-full bg-red-100 text-red-700 font-semibold dark:bg-red-400/20 dark:text-red-100">
                          {activeInteraction.task.task.status.state}
                        </span>
                      </div>
                      <button
                        onClick={handleStartNewInContext}
                        className="text-xs text-red-700 hover:text-red-900 font-medium dark:text-red-200 dark:hover:text-red-100"
                      >
                        Start new conversation
                      </button>
                    </div>
                  )}
                </>
              )}

              {activeInteraction.type === "negotiation" && (
                <div className="flex items-center gap-2 rounded-lg border border-cyan-200 bg-cyan-50 px-4 py-2 dark:border-zinc-700 dark:bg-zinc-800">
                  <div className="h-2 w-2 rounded-full bg-cyan-500 animate-pulse dark:bg-zinc-200" />
                  <span className="text-xs font-semibold text-cyan-900 dark:text-zinc-100">
                    Agent is negotiating (no task created yet)
                  </span>
                </div>
              )}

              {activeInteraction.type === "context" && (
                <div className="flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-4 py-2 dark:border-zinc-700 dark:bg-zinc-800">
                  <span className="text-xs text-slate-600 dark:text-zinc-300">
                    Sending message in context {shortenId(contextId!)}
                  </span>
                </div>
              )}

              {activeInteraction.type === "new" && (
                <div className="flex items-center gap-2 rounded-lg border border-blue-200 bg-blue-50 px-4 py-2 dark:border-zinc-700 dark:bg-zinc-800">
                  <span className="text-xs text-blue-700 font-medium dark:text-zinc-100">
                    Starting new conversation
                  </span>
                </div>
              )}
            </div>

            <form onSubmit={e => handleSendMessage(e)} className="flex gap-3">
              <textarea
                rows={3}
                className="flex-1 rounded-xl border border-slate-300 px-4 py-3 text-sm text-slate-900 placeholder-slate-400 focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500/20 resize-none disabled:bg-slate-100 disabled:cursor-not-allowed dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100 dark:placeholder-zinc-500"
                placeholder={
                  activeInteraction.type === "task" && !activeInteraction.canReceiveInput
                    ? "This task is not waiting for input"
                    : contextId
                    ? "Send a message..."
                    : "Start a new conversation..."
                }
                value={inputValue}
                onChange={e => setInputValue(e.target.value)}
                disabled={isStreaming || (activeInteraction.type === "task" && !activeInteraction.canReceiveInput)}
                onKeyDown={e => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    handleSendMessage(e);
                  }
                }}
              />
              <button
                type="submit"
                className="self-end rounded-xl bg-blue-600 px-6 py-3 text-sm font-semibold text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 transition-colors dark:bg-blue-500 dark:hover:bg-blue-400"
                disabled={
                  isStreaming ||
                  !inputValue.trim() ||
                  (activeInteraction.type === "task" && !activeInteraction.canReceiveInput)
                }
              >
                {isStreaming ? "Sending..." : "Send"}
              </button>
            </form>
            <p className="mt-2 text-xs text-slate-500 dark:text-zinc-400">
              Press Enter to send, Shift+Enter for new line
            </p>
          </div>
        </div>

        {/* Sidebar */}
        <aside className="w-80 h-full border-l border-slate-200 bg-white flex flex-col overflow-hidden dark:border-zinc-800 dark:bg-zinc-900">
          {/* Context selector */}
          <div className="border-b border-slate-200 px-4 py-4 dark:border-zinc-800">
            <p className="text-xs font-semibold uppercase tracking-wide text-slate-600 mb-3 dark:text-zinc-300">
              Context
            </p>
            <div className="space-y-2">
              <button
                onClick={handleNewContext}
                className="w-full rounded-lg bg-blue-600 px-4 py-2 text-sm font-semibold text-white hover:bg-blue-700 transition-colors dark:bg-blue-500 dark:hover:bg-blue-400"
              >
                + New Context
              </button>
              {availableContexts.length > 0 && (
                <select
                  value={contextId || ""}
                  onChange={e => handleSelectContext(e.target.value)}
                  className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm text-slate-900 focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500/20 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                >
                  <option value="">Select existing context...</option>
                  {availableContexts.map(ctx => (
                    <option key={ctx} value={ctx}>
                      {shortenId(ctx)}
                    </option>
                  ))}
                </select>
              )}
              {contextId && (
                <p className="text-xs text-slate-500 dark:text-zinc-400">
                  Active: {shortenId(contextId)}
                </p>
              )}
            </div>
          </div>

          {/* Task list */}
          <div className="flex-1 overflow-y-auto px-4 py-4">
            <p className="text-xs font-semibold uppercase tracking-wide text-slate-600 mb-3 dark:text-zinc-300">
              Tasks ({tasks.length})
            </p>
            {tasks.length === 0 ? (
              <p className="text-xs text-slate-500 dark:text-zinc-400">
                {contextId ? "No tasks yet" : "Select or create a context"}
              </p>
            ) : (
              <div className="space-y-2">
                {tasks.map(({ task }) => {
                  const stateColors: Record<string, string> = {
                    "working": "bg-amber-100 text-amber-700 border-amber-200 dark:bg-amber-400/20 dark:text-amber-100 dark:border-amber-400/30",
                    "input-required": "bg-purple-100 text-purple-700 border-purple-200 dark:bg-purple-400/20 dark:text-purple-100 dark:border-purple-400/30",
                    "completed": "bg-emerald-100 text-emerald-700 border-emerald-200 dark:bg-emerald-400/20 dark:text-emerald-100 dark:border-emerald-400/30",
                    "failed": "bg-red-100 text-red-700 border-red-200 dark:bg-red-400/20 dark:text-red-100 dark:border-red-400/30",
                    "submitted": "bg-blue-100 text-blue-700 border-blue-200 dark:bg-blue-400/20 dark:text-blue-100 dark:border-blue-400/30",
                    "canceled": "bg-slate-100 text-slate-700 border-slate-200 dark:bg-zinc-700/40 dark:text-zinc-200 dark:border-zinc-600",
                    "rejected": "bg-orange-100 text-orange-700 border-orange-200 dark:bg-orange-400/20 dark:text-orange-100 dark:border-orange-400/30",
                  };
                  const colorClass = stateColors[task.status.state] ?? stateColors["submitted"];

                  const needsInput = task.status.state === "input-required";

                  return (
                    <div
                      key={task.id}
                      className="rounded-lg border border-slate-200 bg-slate-50 p-3 hover:bg-slate-100 transition-colors dark:border-zinc-700 dark:bg-zinc-800 dark:hover:bg-zinc-700"
                    >
                      <div className="flex items-start justify-between gap-2 mb-2">
                        <div className="flex-1 min-w-0">
                          <p className="text-xs font-mono text-slate-600 truncate dark:text-zinc-300">
                            {shortenId(task.id)}
                          </p>
                          <span className={`mt-1 inline-block rounded-full border px-2 py-0.5 text-[10px] font-semibold ${colorClass}`}>
                            {task.status.state}
                          </span>
                        </div>
                      </div>
                      <div className="flex gap-2">
                        <button
                          onClick={() =>
                            void (needsInput
                              ? handleResumeTask(task.id)
                              : handleSubscribeTask(task.id))
                          }
                          className="flex-1 rounded-lg bg-purple-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-purple-700 transition-colors dark:bg-purple-500 dark:hover:bg-purple-400"
                        >
                          {needsInput ? "Resume" : "View"}
                        </button>
                        <button
                          onClick={() => void handleInspectTask(task.id)}
                          disabled={inspecting}
                          className="flex-1 rounded-lg border border-slate-300 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-200 disabled:opacity-50 transition-colors dark:border-zinc-600 dark:text-zinc-200 dark:hover:bg-zinc-700"
                        >
                          Inspect
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </aside>
      </div>

      {/* Task inspection modal */}
      {inspectedTask && (
        <div className="fixed inset-0 bg-black/50 dark:bg-black/70 flex items-center justify-center p-6 z-50">
          <div className="bg-white dark:bg-zinc-900 rounded-2xl shadow-2xl max-w-4xl w-full max-h-[80vh] overflow-hidden flex flex-col border border-slate-200 dark:border-zinc-800">
            <div className="border-b border-slate-200 px-6 py-4 flex items-center justify-between dark:border-zinc-800">
              <div>
                <h3 className="text-lg font-semibold text-slate-900 dark:text-zinc-100">
                  Task {shortenId(inspectedTask.taskId)}
                </h3>
                <p className="text-xs text-slate-500 dark:text-zinc-400">Detailed history</p>
              </div>
              <button
                onClick={() => setInspectedTask(null)}
                className="text-slate-400 hover:text-slate-600 text-2xl leading-none dark:text-zinc-500 dark:hover:text-zinc-300"
              >
                ×
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-6 space-y-4">
              {inspectedTask.task && (
                <div className="grid gap-3 rounded-xl border border-slate-200 bg-slate-50 p-4 text-sm text-slate-600 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300">
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-slate-800 dark:text-zinc-100">Context</span>
                    <span className="font-mono text-xs dark:text-zinc-300">
                      {shortenId(inspectedTask.task.contextId)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-slate-800 dark:text-zinc-100">Status</span>
                    <span className="text-xs rounded-full bg-slate-200 px-2 py-0.5 uppercase tracking-wide text-slate-700 dark:bg-zinc-700 dark:text-zinc-200">
                      {inspectedTask.task.status.state}
                    </span>
                  </div>
                  {inspectedTask.task.artifacts?.length ? (
                    <div>
                      <span className="font-semibold text-slate-800 block mb-2 dark:text-zinc-100">
                        Artifacts ({inspectedTask.task.artifacts.length})
                      </span>
                      <div className="space-y-2">
                        {inspectedTask.task.artifacts.map(artifact => (
                          <ArtifactPreview key={artifact.artifactId} artifact={artifact} />
                        ))}
                      </div>
                    </div>
                  ) : null}
                </div>
              )}

              <div>
                <p className="text-xs font-semibold uppercase tracking-wide text-slate-600 mb-3 dark:text-zinc-400">
                  Event History
                </p>
                <div className="space-y-3 rounded-xl border border-slate-200 bg-slate-50 p-4 max-h-96 overflow-y-auto dark:border-zinc-700 dark:bg-zinc-800">
                  {inspectedTask.entries.map((entry) => {
                    const isUser = entry.type === "user";
                    return (
                      <div key={entry.id} className="text-sm text-slate-700 dark:text-zinc-200">
                        <div className="flex items-center gap-2 mb-1">
                          <span className="text-xs font-semibold text-slate-700 dark:text-zinc-300">
                            {entry.title}
                          </span>
                          {entry.badge && (
                            <span className="text-[10px] uppercase tracking-wide px-2 py-0.5 rounded-full bg-slate-200 text-slate-600 font-semibold dark:bg-zinc-600 dark:text-zinc-200">
                              {entry.badge}
                            </span>
                          )}
                          <span className="text-[11px] text-slate-400 dark:text-zinc-500">
                            {new Date(entry.timestamp).toLocaleTimeString()}
                          </span>
                        </div>
                        {entry.body && (
                          <p className={`text-sm ${isUser ? "text-blue-700 dark:text-blue-200" : "text-slate-600 dark:text-zinc-300"} whitespace-pre-wrap`}>
                            {entry.body}
                          </p>
                        )}
                        {entry.subtext && <p className="text-xs text-slate-500 mt-1 dark:text-zinc-400">{entry.subtext}</p>}
                        {entry.artifact && (
                          <div className="mt-2">
                            <ArtifactPreview artifact={entry.artifact} />
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
      {showAgentInfo && (
        <div className="fixed inset-0 bg-black/40 dark:bg-black/70 flex items-center justify-center p-6 z-40">
          <div className="w-full max-w-2xl rounded-2xl bg-white dark:bg-zinc-900 shadow-2xl overflow-hidden border border-slate-200 dark:border-zinc-800">
            <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4 dark:border-zinc-800">
              <div>
                <h2 className="text-lg font-semibold text-slate-900 dark:text-zinc-100">Agent Information</h2>
                <p className="text-xs text-slate-500 dark:text-zinc-400">Details from the published AgentCard</p>
              </div>
              <button
                onClick={() => setShowAgentInfo(false)}
                className="text-slate-400 hover:text-slate-600 text-2xl leading-none dark:text-zinc-500 dark:hover:text-zinc-300"
              >
                ×
              </button>
            </div>
            <div className="max-h-[70vh] overflow-y-auto px-6 py-4 space-y-6">
              <div className="rounded-lg border border-slate-200 bg-slate-50 p-4 text-sm text-slate-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300">
                <p className="text-xs font-semibold uppercase tracking-wide text-slate-500 mb-1 dark:text-zinc-400">
                  Agent Card URL
                </p>
                <a
                  href={detail.cardUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="text-blue-600 hover:text-blue-800 break-all dark:text-blue-300 dark:hover:text-blue-200"
                >
                  {typeof window !== "undefined"
                    ? `${window.location.origin}${detail.cardUrl}`
                    : detail.cardUrl}
                </a>
              </div>

              <div>
                <p className="text-xs font-semibold uppercase tracking-wide text-slate-600 mb-3 dark:text-zinc-400">
                  Skills ({detail.skills.length})
                </p>
                {detail.skills.length === 0 ? (
                  <p className="text-sm text-slate-500 dark:text-zinc-400">
                    This agent has no published skills.
                  </p>
                ) : (
                  <div className="space-y-3">
                    {detail.skills.map(skill => (
                      <div
                        key={skill.id}
                        className="rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-zinc-700 dark:bg-zinc-800"
                      >
                        <div className="flex items-start justify-between gap-2 mb-2">
                          <div>
                            <p className="text-sm font-semibold text-slate-900 dark:text-zinc-100">{skill.name}</p>
                            <p className="text-xs text-slate-500 font-mono dark:text-zinc-400">{skill.id}</p>
                          </div>
                        </div>
                        {skill.description && (
                          <p className="text-sm text-slate-700 mb-2 whitespace-pre-wrap dark:text-zinc-300">
                            {skill.description}
                          </p>
                        )}
                        {skill.examples && skill.examples.length > 0 && (
                          <div className="rounded-lg bg-white border border-slate-200 px-3 py-2 dark:bg-zinc-900 dark:border-zinc-700">
                            <p className="text-xs font-semibold text-slate-600 mb-1 dark:text-zinc-400">Examples</p>
                            <ul className="list-disc pl-5 space-y-1 text-sm text-slate-700 dark:text-zinc-300">
                              {skill.examples.map((example, idx) => (
                                <li key={idx}>{example}</li>
                              ))}
                            </ul>
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
              <div className="flex justify-end gap-2 border-t border-slate-200 pt-3 dark:border-zinc-800">
                <button
                  onClick={() => setShowAgentInfo(false)}
                  className="rounded-lg border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100 transition-colors dark:border-zinc-600 dark:text-zinc-200 dark:hover:bg-zinc-800"
                >
                  Close
                </button>
                <a
                  href={detail.cardUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-semibold text-white hover:bg-blue-700 transition-colors dark:bg-blue-500 dark:hover:bg-blue-400"
                >
                  Open Agent Card
                </a>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// Event conversion logic
function convertEventToEntry(event: A2AStreamEvent): ConsoleEntry | null {
  const timestamp = new Date().toISOString();

  if (isMessageEvent(event)) {
    const isNegotiation = !event.taskId;

    if (event.role === "agent") {
      const textParts = event.parts
        .filter(p => "text" in p && p.text)
        .map(p => ("text" in p && p.text) || "")
        .filter(Boolean);
      const dataParts = event.parts
        .filter(p => "data" in p && p.data)
        .map(p => ("data" in p && p.data ? JSON.stringify(p.data, null, 2) : ""))
        .filter(Boolean);
      const combinedBody = [...textParts, ...dataParts].join("\n\n");
      const fileParts = event.parts.filter(p => "file" in p && p.file);

      return {
        id: event.messageId || `msg-${Date.now()}`,
        type: isNegotiation ? "negotiation" : "agent",
        title: isNegotiation ? "Agent (Negotiation)" : "Agent",
        body: combinedBody || undefined,
        subtext:
          fileParts.length > 0
            ? `${fileParts.length} file part${fileParts.length > 1 ? "s" : ""} attached`
            : undefined,
        contextId: event.contextId,
        taskId: event.taskId,
        timestamp: timestamp,
        badge: isNegotiation ? "Negotiation" : undefined,
      };
    }
  }

  if (isStatusUpdateEvent(event)) {
    const textPart = event.status.message?.parts.find(p => p.kind === "text" && "text" in p);
    const defaultMessages: Record<string, string> = {
      "input-required": "Awaiting additional input",
      "working": "Task is in progress",
      "completed": "Task completed",
      "failed": "Task failed",
      "rejected": "Task was rejected",
      "canceled": "Task was canceled",
    };
    const statusText =
      (textPart && "text" in textPart ? textPart.text : undefined) ??
      defaultMessages[event.status.state] ??
      undefined;
    const statusTimestamp = event.status.timestamp ?? `${Date.now()}`;
    return {
      id: `status-${event.taskId}-${statusTimestamp}`,
      type: "status",
      title: `Status: ${event.status.state}`,
      body: statusText,
      taskId: event.taskId,
      contextId: event.contextId,
      timestamp: event.status.timestamp ?? timestamp,
      badge: event.status.state,
    };
  }

  if (isArtifactUpdateEvent(event)) {
    const artifactId =
      event.artifact.artifactId ||
      `${event.taskId}-${event.artifact.name || "artifact"}-${Date.now()}`;
    return {
      id: `artifact-${artifactId}-${event.append ? "append" : "replace"}`,
      type: "artifact",
      title: "Artifact Update",
      body: event.artifact.name || "Unnamed artifact",
      subtext: event.artifact.description
        ? event.artifact.description
        : event.append
        ? "Appended chunk"
        : undefined,
      artifact: event.artifact,
      taskId: event.taskId,
      contextId: event.contextId,
      timestamp: timestamp,
    };
  }

  if (isTaskEvent(event)) {
    const hasArtifacts = event.artifacts && event.artifacts.length > 0;
    const isCreation = event.status.state === "submitted" || event.status.state === "working";
    const textPart = event.status.message?.parts.find(p => p.kind === "text" && "text" in p);

    if (!isCreation) {
      return null;
    }

    const entry: ConsoleEntry = {
      id: event.id,
      type: "task",
      title: isCreation ? "Task Created" : "Task Summary",
      body: textPart && "text" in textPart ? textPart.text : undefined,
      taskId: event.id,
      contextId: event.contextId,
      timestamp: event.status.timestamp ?? timestamp,
      badge: isCreation ? "Task Created" : "Task Summary",
    };

    if (hasArtifacts && event.artifacts) {
      entry.artifact = event.artifacts[0];
      if (event.artifacts.length > 1) {
        entry.subtext = `${event.artifacts.length} artifacts generated`;
      }
    }

    return entry;
  }

  return null;
}

function isMessageEvent(event: A2AStreamEvent): event is Message {
  return "kind" in event && event.kind === "message";
}

function isTaskEvent(event: A2AStreamEvent): event is A2ATask {
  return "kind" in event && event.kind === "task";
}

function isStatusUpdateEvent(event: A2AStreamEvent): event is TaskStatusUpdateEvent {
  return "kind" in event && event.kind === "status-update";
}

function isArtifactUpdateEvent(event: A2AStreamEvent): event is TaskArtifactUpdateEvent {
  return "kind" in event && event.kind === "artifact-update";
}
