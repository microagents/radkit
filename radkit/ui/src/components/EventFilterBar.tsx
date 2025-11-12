import type { ConsoleEntryType } from "./Console";

export interface EventFilters {
  types: Set<ConsoleEntryType>;
  searchQuery: string;
}

interface EventFilterBarProps {
  filters: EventFilters;
  onFiltersChange: (filters: EventFilters) => void;
}

const FILTER_OPTIONS: { value: ConsoleEntryType; label: string; color: string }[] = [
  { value: "user", label: "User", color: "bg-sky-500/10 text-sky-300 border-sky-500/30" },
  { value: "agent", label: "Agent", color: "bg-emerald-500/10 text-emerald-300 border-emerald-500/30" },
  { value: "negotiation", label: "Negotiation", color: "bg-cyan-500/10 text-cyan-300 border-cyan-500/30" },
  { value: "status", label: "Status", color: "bg-amber-500/10 text-amber-300 border-amber-500/30" },
  { value: "artifact", label: "Artifacts", color: "bg-violet-500/10 text-violet-300 border-violet-500/30" },
  { value: "task", label: "Tasks", color: "bg-blue-500/10 text-blue-300 border-blue-500/30" },
];

export default function EventFilterBar({ filters, onFiltersChange }: EventFilterBarProps) {
  const toggleType = (type: ConsoleEntryType) => {
    const newTypes = new Set(filters.types);
    if (newTypes.has(type)) {
      newTypes.delete(type);
    } else {
      newTypes.add(type);
    }
    onFiltersChange({ ...filters, types: newTypes });
  };

  const handleSearchChange = (query: string) => {
    onFiltersChange({ ...filters, searchQuery: query });
  };

  const clearFilters = () => {
    onFiltersChange({
      types: new Set(FILTER_OPTIONS.map(opt => opt.value)),
      searchQuery: "",
    });
  };

  const allSelected = filters.types.size === FILTER_OPTIONS.length;
  const hasActiveFilters = !allSelected || filters.searchQuery.length > 0;

  return (
    <div className="space-y-3 rounded-2xl bg-slate-900/50 p-4 border border-slate-800">
      <div className="flex items-center justify-between">
        <p className="text-xs font-semibold uppercase tracking-wide text-slate-400">
          Filter Events
        </p>
        {hasActiveFilters && (
          <button
            type="button"
            onClick={clearFilters}
            className="text-xs text-slate-400 hover:text-slate-300"
          >
            Clear all
          </button>
        )}
      </div>

      <div className="flex flex-wrap gap-2">
        {FILTER_OPTIONS.map(option => {
          const isActive = filters.types.has(option.value);
          return (
            <button
              key={option.value}
              type="button"
              onClick={() => toggleType(option.value)}
              className={`rounded-full border px-3 py-1 text-xs font-semibold transition ${
                isActive
                  ? option.color
                  : "border-slate-700 bg-slate-800/50 text-slate-500 hover:border-slate-600"
              }`}
            >
              {option.label}
            </button>
          );
        })}
      </div>

      <div className="relative">
        <input
          type="text"
          placeholder="Search events..."
          value={filters.searchQuery}
          onChange={e => handleSearchChange(e.target.value)}
          className="w-full rounded-xl border border-slate-700 bg-slate-800/50 px-4 py-2 text-sm text-slate-200 placeholder-slate-500 focus:border-cyan-500 focus:outline-none focus:ring-1 focus:ring-cyan-500"
        />
        {filters.searchQuery && (
          <button
            type="button"
            onClick={() => handleSearchChange("")}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-300"
            title="Clear search"
          >
            <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}
