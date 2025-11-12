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
import type { AgentInfo } from "../types/AgentInfo";

// Union type for streaming events (SDK doesn't export this)
export type A2AStreamEvent =
  | Message
  | Task
  | TaskStatusUpdateEvent
  | TaskArtifactUpdateEvent;

// Base URL for API calls (relative to current origin)
const API_BASE = "";

// Cache of A2A clients per agent
const clientCache = new Map<string, Promise<A2AClient>>();

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
async function getA2AClient(agentId: string): Promise<A2AClient> {
  if (!clientCache.has(agentId)) {
    const cardUrl = `${API_BASE}/${agentId}/.well-known/agent-card.json`;
    console.log("[A2A Client] Creating client for:", cardUrl);

    const clientPromise = A2AClient.fromCardUrl(cardUrl)
      .then(client => {
        console.log("[A2A Client] Client created successfully");
        return client;
      })
      .catch(error => {
        console.error("[A2A Client] Failed to create client:", error);
        throw error;
      });

    clientCache.set(agentId, clientPromise);
  }

  return clientCache.get(agentId)!;
}

/**
 * List all available agents
 * Fetches from the /ui/agents discovery endpoint (dev-ui only)
 */
export async function listAgents(): Promise<AgentInfo[]> {
  const response = await fetch(`${API_BASE}/ui/agents`);
  if (!response.ok) {
    throw new Error(`Failed to fetch agents: ${response.statusText}`);
  }
  return response.json();
}

/**
 * Fetch agent card from the A2A protocol endpoint
 */
export async function getAgentCard(agentId: string): Promise<AgentCard> {
  const client = await getA2AClient(agentId);
  const card = await client.getAgentCard();
  console.log("[A2A Client] Agent card:", card);
  return card;
}

/**
 * Send a message to an agent using streaming.
 * Returns an AsyncGenerator that yields events as they arrive.
 */
export async function* sendMessageStream(
  agentId: string,
  options: SendMessageOptions
): AsyncGenerator<A2AStreamEvent, void, undefined> {
  const client = await getA2AClient(agentId);

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
  agentId: string,
  params: TaskIdParams
): AsyncGenerator<A2AStreamEvent, void, undefined> {
  const client = await getA2AClient(agentId);
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
export async function getAgentDetail(agentId: string) {
  const card = await getAgentCard(agentId);

  return {
    id: agentId,
    name: card.name,
    description: card.description,
    version: card.version,
    skills: card.skills || [],
    cardUrl: `/${agentId}/.well-known/agent-card.json`,
  };
}

export async function listContextTasks(
  agentId: string,
  version: string,
  contextId: string
): Promise<TaskSummary[]> {
  const response = await fetch(
    `${API_BASE}/ui/${agentId}/${version}/contexts/${contextId}/tasks`
  );

  if (!response.ok) {
    throw new Error(`Failed to load tasks for context ${contextId}`);
  }

  return response.json();
}

export async function getTaskHistory(
  agentId: string,
  version: string,
  taskId: string
): Promise<TaskHistoryResponse> {
  const response = await fetch(
    `${API_BASE}/ui/${agentId}/${version}/tasks/${taskId}/events`
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
  agentId: string,
  version: string,
  taskId: string
): Promise<import("../types/TaskTransitionsResponse").TaskTransitionsResponse> {
  const response = await fetch(
    `${API_BASE}/ui/${agentId}/${version}/tasks/${taskId}/transitions`
  );

  if (!response.ok) {
    throw new Error(`Failed to load transitions for task ${taskId}`);
  }

  return response.json();
}

export async function listContexts(
  agentId: string,
  version: string
): Promise<string[]> {
  const response = await fetch(
    `${API_BASE}/ui/${agentId}/${version}/contexts`
  );

  if (!response.ok) {
    throw new Error(`Failed to load contexts for agent ${agentId}`);
  }

  return response.json();
}
