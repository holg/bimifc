#!/usr/bin/env bash
# File: scripts/test-ci-locally.sh
# Local CI check for bimifc workspace

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Testing CI Workflow Locally ===${NC}\n"

# Run cargo fmt check
echo -e "${YELLOW}Step 1: Running cargo fmt check...${NC}"
if cargo fmt -- --check; then
    echo -e "${GREEN}✓ cargo fmt passed${NC}\n"
else
    echo -e "${RED}✗ cargo fmt failed${NC}"
    echo -e "${YELLOW}Run 'cargo fmt' to fix formatting issues${NC}\n"
    exit 1
fi

# Run clippy
echo -e "${YELLOW}Step 2: Running clippy...${NC}"
if cargo clippy --workspace --all-targets -- -D warnings; then
    echo -e "${GREEN}✓ clippy passed${NC}\n"
else
    echo -e "${RED}✗ clippy failed${NC}\n"
    exit 1
fi

# Run cargo build
echo -e "${YELLOW}Step 3: Running cargo build...${NC}"
if cargo build --workspace --exclude bimifc-python; then
    echo -e "${GREEN}✓ build passed${NC}\n"
else
    echo -e "${RED}✗ build failed${NC}\n"
    exit 1
fi

# Run cargo doc
echo -e "${YELLOW}Step 4: Running cargo doc...${NC}"
if RUSTDOCFLAGS="-D warnings" cargo doc --workspace --document-private-items --exclude bimifc-python; then
    echo -e "${GREEN}✓ doc generation passed${NC}\n"
else
    echo -e "${RED}✗ doc generation failed${NC}\n"
    exit 1
fi

# Additional checks
echo -e "${YELLOW}Step 5: Running additional checks...${NC}"

# Check cargo-sort
if command -v cargo-sort &> /dev/null; then
    echo "Running cargo-sort check..."
    if cargo-sort -cwg; then
        echo -e "${GREEN}✓ cargo-sort passed${NC}"
    else
        echo -e "${RED}✗ cargo-sort failed${NC}"
        echo -e "${YELLOW}Run 'cargo-sort -wg' to fix Cargo.toml sorting${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ cargo-sort not installed, skipping Cargo.toml sort check${NC}"
    echo -e "${YELLOW}Install with: cargo install cargo-sort${NC}"
fi

# Check taplo
if command -v taplo &> /dev/null; then
    echo "Running taplo format check..."
    if taplo format --check; then
        echo -e "${GREEN}✓ taplo passed${NC}"
    else
        echo -e "${RED}✗ taplo failed${NC}"
        echo -e "${YELLOW}Run 'taplo format' to fix TOML formatting${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ taplo not installed, skipping TOML format check${NC}"
    echo -e "${YELLOW}Install with: cargo install taplo-cli${NC}"
fi

# Check cargo-deny
if command -v cargo-deny &> /dev/null; then
    echo "Running cargo-deny check..."
    if cargo-deny check --hide-inclusion-graph --show-stats; then
        echo -e "${GREEN}✓ cargo-deny passed${NC}"
    else
        echo -e "${RED}✗ cargo-deny failed${NC}"
        exit 1
    fi
else
    echo -e "${YELLOW}⚠ cargo-deny not installed, skipping dependency check${NC}"
    echo -e "${YELLOW}Install with: cargo install cargo-deny${NC}"
fi

# Run tests
echo -e "${YELLOW}Step 6: Running tests...${NC}"
if cargo test --workspace --exclude bimifc-python -- --test-threads=1; then
    echo -e "${GREEN}✓ tests passed${NC}\n"
else
    echo -e "${RED}✗ tests failed${NC}\n"
    exit 1
fi

echo -e "\n${GREEN}=== All CI checks passed! ===${NC}"
echo -e "${GREEN}Your code is ready to be pushed.${NC}\n"
