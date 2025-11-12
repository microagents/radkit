import { Link } from "react-router";

interface NavigationProps {
  theme: "light" | "dark";
  onToggleTheme: () => void;
}

export default function Navigation({ theme, onToggleTheme }: NavigationProps) {
  const isDark = theme === "dark";
  const icon = isDark ? (
    <svg viewBox="0 0 24 24" className="h-4 w-4 fill-current" aria-hidden="true">
      <path d="M12 4a1 1 0 0 1 1 1v1.05a1 1 0 1 1-2 0V5a1 1 0 0 1 1-1Zm5.657 2.343a1 1 0 0 1 1.414 0l.744.744a1 1 0 1 1-1.414 1.414l-.744-.744a1 1 0 0 1 0-1.414ZM12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8Zm7 3a1 1 0 0 1 1 1v1.05a1 1 0 1 1-2 0V12a1 1 0 0 1 1-1Zm-1.485 6.263a1 1 0 0 1 0 1.414l-.744.744a1 1 0 0 1-1.414-1.414l.744-.744a1 1 0 0 1 1.414 0ZM12 18a1 1 0 0 1 1 1v1.05a1 1 0 1 1-2 0V19a1 1 0 0 1 1-1Zm-5.657-2.343a1 1 0 0 1 0 1.414l-.744.744a1 1 0 0 1-1.414-1.414l.744-.744a1 1 0 0 1 1.414 0ZM5 11a1 1 0 0 1 1 1v1.05a1 1 0 1 1-2 0V12a1 1 0 0 1 1-1Zm1.343-4.657a1 1 0 0 1 0 1.414l-.744.744a1 1 0 1 1-1.414-1.414l.744-.744a1 1 0 0 1 1.414 0Z" />
    </svg>
  ) : (
    <svg viewBox="0 0 24 24" className="h-4 w-4 fill-current" aria-hidden="true">
      <path d="M12 3.25a.75.75 0 0 1 .75.75v1a.75.75 0 0 1-1.5 0V4a.75.75 0 0 1 .75-.75Zm6.01 2.74a.75.75 0 0 1 1.06 1.06l-.71.7a.75.75 0 0 1-1.06-1.06l.71-.7Zm-12.02 0 .71.7a.75.75 0 1 1-1.06 1.06l-.71-.7a.75.75 0 0 1 1.06-1.06ZM12 7.5a4.5 4.5 0 1 1 0 9 4.5 4.5 0 0 1 0-9Zm9 .25a.75.75 0 0 1 .75.75v1a.75.75 0 0 1-1.5 0V8.5a.75.75 0 0 1 .75-.75Zm-18 0a.75.75 0 0 1 .75.75v1a.75.75 0 0 1-1.5 0V8.5a.75.75 0 0 1 .75-.75Zm15.25 6.25a.75.75 0 0 1 1.25.83l-.44.88a.75.75 0 1 1-1.34-.67l.53-1.04Zm-12.5 0 .53 1.04a.75.75 0 0 1-1.34.67l-.44-.88a.75.75 0 1 1 1.25-.83ZM12 17.75a.75.75 0 0 1 .75.75v1a.75.75 0 1 1-1.5 0v-1a.75.75 0 0 1 .75-.75Z" />
    </svg>
  );

  return (
    <nav className="bg-white border-b border-gray-200 shadow-sm dark:bg-zinc-900 dark:border-zinc-800">
      <div className="mx-auto w-full px-3 sm:px-4">
        <div className="flex h-12 items-center justify-between">
          <Link to="/" className="flex items-center">
            <span className="text-lg font-semibold text-blue-600 dark:text-blue-300">Radkit</span>
            <span className="ml-2 text-xs uppercase tracking-wide text-gray-500 dark:text-zinc-400">
              Dev UI
            </span>
          </Link>
          <button
            type="button"
            onClick={onToggleTheme}
            className="inline-flex items-center justify-center rounded-full border border-gray-300 bg-white p-2 text-gray-700 transition-colors hover:bg-gray-100 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700"
            aria-label={isDark ? "Switch to light mode" : "Switch to dark mode"}
          >
            {icon}
          </button>
        </div>
      </div>
    </nav>
  );
}
