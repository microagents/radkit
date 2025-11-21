import { A2AClient } from "@a2a-js/sdk/client";
import type {
  MessageSendParams,
  AgentCard,
  Message,
  Task,
  TaskStatusUpdateEvent,
  TaskArtifactUpdateEvent,
  TaskIdParams,
} from "@a2a-js/sdk";
import { v4 as uuidv4 } from "uuid";
// Union type for streaming events (SDK doesn't export this)
export type A2AStreamEvent =
  | Message
  | Task
  | TaskStatusUpdateEvent
  | TaskArtifactUpdateEvent;

// Base URL for API calls (relative to current origin)
const API_BASE = "";

// Cache of the runtime client
let cachedClient: Promise<A2AClient> | null = null;

export interface SendMessageOptions {
  messageText: string;
  contextId?: string;
  taskId?: string;
}

export interface TaskSummary {
  task: Task;
  skill_id?: string;
  pending_slot?: unknown;
}

export interface UiTaskEvent {
  result: A2AStreamEvent;
  is_final: boolean;
}

export interface TaskHistoryResponse {
  events: UiTaskEvent[];
  task?: Task;
}

/**
 * Get or create an A2AClient for the specified agent
 */
async function getA2AClient(): Promise<A2AClient> {
  if (!cachedClient) {
    const cardUrl = `${API_BASE}/.well-known/agent-card.json`;
    console.log("[A2A Client] Creating client for:", cardUrl);
    cachedClient = A2AClient.fromCardUrl(cardUrl)
      .then(client => {
        console.log("[A2A Client] Client created successfully");
        return client;
      })
      .catch(error => {
        console.error("[A2A Client] Failed to create client:", error);
        cachedClient = null;
        throw error;
      });
  }

  return cachedClient;
}

/**
 * Fetch agent card from the A2A protocol endpoint
 */
export async function getAgentCard(): Promise<AgentCard> {
  const client = await getA2AClient();
  const card = await client.getAgentCard();
  console.log("[A2A Client] Agent card:", card);
  return card;
}

/**
 * Send a message to an agent using streaming.
 * Returns an AsyncGenerator that yields events as they arrive.
 */
export async function* sendMessageStream(
  options: SendMessageOptions
): AsyncGenerator<A2AStreamEvent, void, undefined> {
  const client = await getA2AClient();

  const message: Message = {
    messageId: uuidv4(),
    role: "user",
    parts: [{ kind: "text", text: options.messageText }],
    kind: "message",
    contextId: options.contextId,
    taskId: options.taskId,
  };

  const sendParams: MessageSendParams = { message };
  console.log("[A2A Client] Sending message:", sendParams);

  try {
    for await (const event of client.sendMessageStream(sendParams)) {
      console.log("[A2A Client] Received event:", event);
      yield event;
    }
  } catch (error) {
    console.error("[A2A Client] Stream error:", error);
    throw error;
  }
}

export async function* resubscribeTaskStream(
  params: TaskIdParams
): AsyncGenerator<A2AStreamEvent, void, undefined> {
  const client = await getA2AClient();
  try {
    for await (const event of client.resubscribeTask(params)) {
      yield event as A2AStreamEvent;
    }
  } catch (error) {
    console.error("[A2A Client] Resubscribe stream error:", error);
    throw error;
  }
}

/**
 * Get agent detail including full metadata
 */
export async function getAgentDetail() {
  const card = await getAgentCard();

  return {
    name: card.name,
    description: card.description,
    version: card.version,
    skills: card.skills || [],
    cardUrl: "/.well-known/agent-card.json",
  };
}

export async function listContextTasks(
  contextId: string
): Promise<TaskSummary[]> {
  const response = await fetch(
    `${API_BASE}/ui/contexts/${contextId}/tasks`
  );

  if (!response.ok) {
    throw new Error(`Failed to load tasks for context ${contextId}`);
  }

  return response.json();
}

export async function getTaskHistory(
  taskId: string
): Promise<TaskHistoryResponse> {
  const response = await fetch(
    `${API_BASE}/ui/tasks/${taskId}/events`
  );

  if (!response.ok) {
    throw new Error(`Failed to load history for task ${taskId}`);
  }

  const payload = await response.json();
  return {
    events: (payload.events ?? []) as UiTaskEvent[],
    task: payload.task,
  };
}

export async function getTaskTransitions(
  taskId: string
): Promise<import("../types/TaskTransitionsResponse").TaskTransitionsResponse> {
  const response = await fetch(
    `${API_BASE}/ui/tasks/${taskId}/transitions`
  );

  if (!response.ok) {
    throw new Error(`Failed to load transitions for task ${taskId}`);
  }

  return response.json();
}

export async function listContexts(): Promise<string[]> {
  const response = await fetch(`${API_BASE}/ui/contexts`);

  if (!response.ok) {
    throw new Error("Failed to load contexts");
  }

  return response.json();
}
