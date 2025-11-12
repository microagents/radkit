import { useCallback, useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";
import Navigation from "./components/Navigation";

export default function App() {
  const location = useLocation();
  const isAgentDetailRoute = location.pathname.startsWith("/agents/");
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    if (typeof window === "undefined") {
      return "light";
    }
    const stored = window.localStorage.getItem("radkit-ui-theme");
    if (stored === "dark" || stored === "light") {
      return stored;
    }
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  });
  const toggleTheme = useCallback(() => {
    setTheme(prev => (prev === "dark" ? "light" : "dark"));
  }, []);

  useEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    const root = document.documentElement;
    root.classList.toggle("dark", theme === "dark");
    root.dataset.theme = theme;
    if (typeof window !== "undefined") {
      window.localStorage.setItem("radkit-ui-theme", theme);
    }
  }, [theme]);

  const sharedPadding = "px-4 sm:px-6 lg:px-8 py-8 flex flex-col";
  const agentDetailMain = `${sharedPadding} flex-1 min-h-0 w-full overflow-hidden`;
  const defaultMain = `${sharedPadding} flex-1 w-full max-w-7xl mx-auto`;
  const mainClasses = isAgentDetailRoute ? agentDetailMain : defaultMain;

  return (
    <div className="min-h-screen h-screen bg-gray-50 dark:bg-zinc-950 flex flex-col">
      <Navigation theme={theme} onToggleTheme={toggleTheme} />
      <main className={mainClasses}>
        <Outlet />
      </main>
    </div>
  );
}
