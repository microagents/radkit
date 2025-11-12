import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router";
import App from "./App";
import Home, { loader as homeLoader } from "./routes/home";
import AgentDetail, { loader as agentLoader } from "./routes/agent-detail";
import "./index.css";

// Create router with React Router Data Mode
const router = createBrowserRouter([
  {
    path: "/",
    Component: App,
    children: [
      {
        index: true,
        Component: Home,
        loader: homeLoader,
      },
      {
        path: "agents/:agentId",
        Component: AgentDetail,
        loader: agentLoader,
      },
    ],
  },
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>
);
