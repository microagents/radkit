import type { AgentInfo } from "../types/AgentInfo";

interface AgentCardProps {
  agent: AgentInfo;
}

export default function AgentCard({ agent }: AgentCardProps) {
  return (
    <div className="bg-white rounded-lg shadow-md p-6 hover:shadow-lg transition-shadow dark:bg-zinc-900 dark:border dark:border-zinc-800">
      <h2 className="text-2xl font-bold text-gray-900 mb-2 dark:text-zinc-100">{agent.name}</h2>
      {agent.description && (
        <p className="text-gray-600 mb-4 dark:text-zinc-300">{agent.description}</p>
      )}
      <div className="flex items-center justify-between text-sm text-gray-500 dark:text-zinc-400">
        <span className="font-mono bg-gray-100 px-2 py-1 rounded dark:bg-zinc-800 dark:text-zinc-200">
          {agent.id}
        </span>
        <span className="font-mono bg-blue-100 text-blue-800 px-2 py-1 rounded dark:bg-zinc-800 dark:text-zinc-200">
          v{agent.version}
        </span>
      </div>
      {agent.skill_count > 0 && (
        <div className="mt-4">
          <span className="text-sm text-gray-500 dark:text-zinc-400">
            {agent.skill_count} skill{agent.skill_count !== 1 ? "s" : ""}
          </span>
        </div>
      )}
    </div>
  );
}
