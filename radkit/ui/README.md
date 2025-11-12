# Radkit UI Development Guide

The Radkit Dev UI is a React + TypeScript application using React Router Data Mode for building and testing AI agents.

## Development Workflow

### Simple Approach (No Hot Reload)

**Run Radkit with dev-ui:**
```bash
cargo run --example hr_agent --features dev-ui
# Visit http://localhost:8080
```

**Make UI changes:**
```bash
cd radkit/ui
# Edit files in src/
npm run build
# Refresh browser at http://localhost:8080
```

**Commit changes:**
```bash
git add dist/
git commit -m "feat(ui): update UI"
```

This is the recommended workflow since it's simple and matches what end users experience.

## End User Experience

End users don't need Node.js or any build tools:

```bash
cargo run --example hr_agent --features dev-ui
# Visit http://localhost:8080
```

The pre-built `dist/` directory is committed to git, so it just works.

## Project Structure

```
radkit/ui/
├── src/
│   ├── main.tsx              # React Router setup with createBrowserRouter
│   ├── App.tsx               # Root layout component
│   ├── routes/               # Route components with loaders/actions
│   │   ├── home.tsx          # Agents list
│   │   └── agent-detail.tsx  # Agent detail + console
│   ├── api/
│   │   └── client.ts         # A2A API client
│   ├── components/           # Reusable UI components
│   │   ├── AgentCard.tsx
│   │   ├── Console.tsx       # Task streaming with SSE
│   │   └── Navigation.tsx
│   └── types/
│       └── generated.ts      # TypeScript types (future: ts-rs)
├── dist/                     # Pre-built production assets (committed!)
├── package.json              # Dependencies
├── vite.config.ts            # Build configuration
├── tsconfig.json             # TypeScript configuration
└── tailwind.config.js        # Tailwind CSS configuration
```

## Available Routes

### A2A Protocol Routes (Backend)
- `/{agent_id}/.well-known/agent-card.json` - Agent metadata
- `/{agent_id}/{version}/rpc` - JSON-RPC endpoint
- `/{agent_id}/{version}/message:stream` - Server-Sent Events streaming
- `/{agent_id}/{version}/tasks/{task_id}/subscribe` - Task resubscription

### UI API Routes (dev-ui only)
- `/ui/agents` - List all registered agents (discovery endpoint)

### UI Routes (Frontend, Client-Side Routing)
- `/` - Agents list (home page)
- `/agents/:agentId` - Agent detail with interactive console

## Technology Stack

- **React 18** - UI library
- **React Router 7 (Data Mode)** - Routing with loaders and actions
- **TypeScript** - Type safety
- **Vite** - Build tool
- **Tailwind CSS** - Styling
- **Server-Sent Events** - Real-time task streaming

## React Router Data Mode

The UI uses React Router's Data Mode pattern:

```typescript
// Routes are configured as objects
const router = createBrowserRouter([
  {
    path: "/",
    Component: App,
    children: [
      {
        index: true,
        Component: Home,
        loader: homeLoader,  // Fetches data before rendering
      },
      {
        path: "agents/:agentId",
        Component: AgentDetail,
        loader: agentLoader,  // Pre-loads agent data
        action: agentAction,  // Handles form submissions
      },
    ],
  },
]);
```

**Benefits:**
- Data loaded before component renders
- Automatic error handling
- Pending UI states with `useNavigation()`
- Form submissions with automatic revalidation

## Common Tasks

### Adding a New Route

1. Create route component in `src/routes/`
2. Add loader function (optional)
3. Add action function (optional)
4. Register route in `src/main.tsx`

Example:
```typescript
// src/routes/settings.tsx
export async function loader() {
  // Fetch data
  return { settings };
}

export default function Settings() {
  const { settings } = useLoaderData();
  return <div>Settings Page</div>;
}

// src/main.tsx
{
  path: "settings",
  Component: Settings,
  loader: settingsLoader,
}
```

### Adding a New API Endpoint

1. Add handler in `radkit/src/runtime/web.rs`
2. Register route in `radkit/src/runtime/mod.rs` under `ui_api_routes`
3. Add client function in `src/api/client.ts`
4. Use in components

### Styling with Tailwind

Tailwind utility classes are available throughout the app:

```tsx
<div className="bg-white rounded-lg shadow-md p-6 hover:shadow-lg">
  <h2 className="text-2xl font-bold text-gray-900">Agent Name</h2>
</div>
```

## Troubleshooting

**Q: My changes aren't showing up**
A: Make sure you ran `npm run build` and refreshed the browser.

**Q: I see "No agents found"**
A: Check that you're running with `--features dev-ui` and that agents are registered.

**Q: Build fails with TypeScript errors**
A: Run `npm install` to ensure dependencies are up to date.

**Q: Where are the built files?**
A: After `npm run build`, check the `dist/` directory. This is what gets served by Rust.

## For Library Maintainers

### Building the UI

```bash
cd radkit/ui
npm install  # First time only
npm run build
```

### Committing Changes

Always commit the `dist/` directory after building:

```bash
git add dist/
git commit -m "feat(ui): your changes"
```

This ensures end users don't need Node.js.

### Development Server (Optional)

If you need hot reload during heavy UI development:

```bash
# Terminal 1: Vite dev server
cd radkit/ui
npm run dev  # Runs on localhost:5173

# Terminal 2: Rust backend
cargo run --example hr_agent --features runtime  # NOT dev-ui!

# Access UI at http://localhost:5173
# API calls will fail - you'd need to add proxy config back
```

**Note:** This setup is more complex and generally not recommended. The simple "build and refresh" approach is usually sufficient.

## Architecture Notes

- **No Server-Side Rendering**: This is a pure SPA (Single Page Application)
- **Same-Origin Serving**: UI and API served from same origin (no CORS issues)
- **Client-Side Routing**: React Router handles all `/agents/*` routes
- **Fallback to index.html**: Axum's `ServeDir` serves `index.html` for unknown routes

## Future Enhancements

- [ ] TypeScript type generation from Rust (ts-rs)
- [ ] Proper agent discovery protocol
- [ ] Task history and replay
- [ ] Settings and configuration UI
- [ ] Authentication and authorization
