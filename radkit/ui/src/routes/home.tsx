import { useLoaderData, Link } from "react-router";
import { listAgents } from "../api/client";
import AgentCard from "../components/AgentCard";
import type { AgentInfo } from "../types/AgentInfo";

// Loader function for React Router Data Mode
export async function loader() {
  const agents = await listAgents();
  return { agents };
}

export default function Home() {
  const { agents } = useLoaderData<{ agents: AgentInfo[] }>();

  return (
    <div className="text-gray-900 dark:text-zinc-100">
      <div className="mb-8">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-zinc-100">Agents</h1>
        <p className="mt-2 text-gray-600 dark:text-zinc-300">
          Available agents in your Radkit runtime
        </p>
      </div>

      {agents.length === 0 ? (
        <div className="bg-white dark:bg-zinc-900 rounded-lg shadow p-12 text-center border border-transparent dark:border-zinc-800">
          <h2 className="text-xl font-semibold text-gray-900 dark:text-white mb-2">
            No agents found
          </h2>
          <p className="text-gray-600 dark:text-zinc-300">
            Make sure your agents are registered with the runtime
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {agents.map((agent) => (
            <Link
              key={agent.id}
              to={`/agents/${agent.id}`}
              className="block hover:scale-105 transition-transform"
            >
              <AgentCard agent={agent} />
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
