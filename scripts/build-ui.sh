#!/usr/bin/env bash
# Build script for Radkit UI
# This script:
# 1. Builds Rust with dev-ui feature (generates TypeScript types via ts-rs)
# 2. Builds the UI with npm
# 3. Runs all necessary checks

set -e  # Exit on error

# Color output
GREEN='\033[0.32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}==> Building Radkit UI${NC}"
echo

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Step 1: Run Rust tests with all features to generate TypeScript types (ts-rs exports during tests)
echo -e "${YELLOW}Step 1/3: Running tests to generate TypeScript types...${NC}"
cd "$PROJECT_ROOT"
cargo test -p radkit --all-features --lib 2>&1 | grep -E "(Running|Compiling|Finished|test result)" || true

# Check if types were generated
TYPES_FILE="$PROJECT_ROOT/radkit/ui/src/types/AgentInfo.ts"
if [ -f "$TYPES_FILE" ]; then
    echo -e "${GREEN}✓ TypeScript types generated at radkit/ui/src/types/AgentInfo.ts${NC}"
else
    echo -e "${YELLOW}⚠ Warning: TypeScript types file not found${NC}"
fi

# Step 2: Install npm dependencies if needed
echo
echo -e "${YELLOW}Step 2/3: Checking npm dependencies...${NC}"
cd "$PROJECT_ROOT/radkit/ui"

if [ ! -d "node_modules" ]; then
    echo "Installing npm dependencies..."
    npm install
else
    echo -e "${GREEN}✓ Dependencies already installed${NC}"
fi

# Step 3: Build the UI
echo
echo -e "${YELLOW}Step 3/3: Building UI...${NC}"
npm run build

echo
echo -e "${GREEN}==> Build complete!${NC}"
echo
echo "The UI is now ready to serve:"
echo "  cargo run --example hr_agent --features dev-ui"
echo
echo "Files generated:"
echo "  - radkit/ui/src/types/AgentInfo.ts (TypeScript types)"
echo "  - radkit/ui/dist/ (Built UI assets)"
echo
echo "Remember to commit both files to git:"
echo "  git add radkit/ui/src/types/AgentInfo.ts radkit/ui/dist/"
