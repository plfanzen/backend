#!/usr/bin/env bash
# SPDX-FileCopyrightText: Aaron Dewes <aaron@nirvati.de>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
YELLOW=$'\033[1;33m'
BLUE=$'\033[0;34m'
NC=$'\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# PIDs of background processes (these are the wrapper subshell PIDs)
MANAGER_PID=""
API_PID=""

# Kill a process and all its children
kill_tree() {
    local pid="$1"
    if [ -n "$pid" ]; then
        # Kill all child processes first
        pkill -TERM -P "$pid" 2>/dev/null || true
        # Then kill the process itself
        kill -TERM "$pid" 2>/dev/null || true
        # Wait a moment for graceful shutdown
        sleep 0.5
        # Force kill if still running
        pkill -KILL -P "$pid" 2>/dev/null || true
        kill -KILL "$pid" 2>/dev/null || true
    fi
}

cleanup() {
    log_info "Shutting down services..."
    
    # Kill any remaining cargo watch and cargo run processes
    pkill -TERM -f "cargo watch.*plfanzen" 2>/dev/null || true
    pkill -TERM -f "cargo run.*plfanzen" 2>/dev/null || true
    pkill -TERM -f "plfanzen-manager" 2>/dev/null || true
    pkill -TERM -f "plfanzen-api" 2>/dev/null || true
    
    # Kill tracked process trees
    kill_tree "$API_PID"
    kill_tree "$MANAGER_PID"
    
    # Wait for cleanup
    sleep 0.5
    
    # Force kill any stragglers
    pkill -KILL -f "cargo watch.*plfanzen" 2>/dev/null || true
    pkill -KILL -f "cargo run.*plfanzen" 2>/dev/null || true
    pkill -KILL -f "plfanzen-manager" 2>/dev/null || true
    pkill -KILL -f "plfanzen-api" 2>/dev/null || true
    
    log_success "Services stopped"
    exit 0
}

trap cleanup SIGINT SIGTERM EXIT

check_dependencies() {
    log_info "Checking dependencies..."
    
    if ! command -v cargo &> /dev/null; then
        log_error "cargo not found. Please install Rust: https://rustup.rs/"
        exit 1
    fi
    
    if ! cargo watch --version &> /dev/null; then
        log_warn "cargo-watch not found. Installing..."
        cargo install cargo-watch
    fi
    
    log_success "Dependencies OK"
}

load_env() {
    local env_file="$PROJECT_ROOT/.env"
    
    if [ -f "$env_file" ]; then
        log_info "Loading environment from .env file..."
        set -a
        # shellcheck source=/dev/null
        source "$env_file"
        set +a
    else
        log_warn "No .env file found. Using environment variables or defaults."
        log_warn "Run './scripts/dev-setup.sh' first to create the .env file."
    fi
    
    # Set defaults if not provided
    export DATABASE_URL="${DATABASE_URL:-postgres://plfanzen:devpassword@localhost:5432/plfanzen}"
    export MANAGER_ENDPOINT="${MANAGER_ENDPOINT:-http://localhost:50051}"
    export REPO_DIR="${REPO_DIR:-/tmp/plfanzen-repo}"
    export GIT_URL="${GIT_URL:-https://github.com/plfanzen/test-ctf.git}"
    export GIT_BRANCH="${GIT_BRANCH:-main}"
    export RUST_LOG="${RUST_LOG:-info}"
    
    # Set kubeconfig for kube-rs to connect to the kind cluster
    export KUBECONFIG="${KUBECONFIG:-$HOME/.kube/config}"
    
    # Set the cluster name for context switching if needed
    local cluster_name="${CLUSTER_NAME:-plfanzen-dev}"
    
    # Ensure kubectl context is set correctly for kube-rs
    if command -v kubectl &> /dev/null; then
        local current_context=$(kubectl config current-context 2>/dev/null || echo "")
        local expected_context="kind-$cluster_name"
        
        if [[ "$current_context" != "$expected_context" ]]; then
            log_warn "Current kubectl context is '$current_context', expected '$expected_context'"
            log_info "Switching to '$expected_context' context..."
            
            if kubectl config use-context "$expected_context" 2>/dev/null; then
                log_success "Switched to '$expected_context' context"
            else
                log_error "Could not switch to '$expected_context' context."
                log_error "Make sure the cluster is running: ./scripts/dev-setup.sh"
                exit 1
            fi
        else
            log_info "Using kubectl context: $current_context"
        fi
    else
        log_error "kubectl not found. Cannot verify Kubernetes connection."
        exit 1
    fi
}

start_manager() {
    log_info "Starting plfanzen-manager with auto-reload..."
    
    cd "$PROJECT_ROOT"
    cargo watch \
        -w crates/manager \
        -x "run -p plfanzen-manager" \
        --why \
        2>&1 | sed "s/^/${GREEN}[manager]${NC} /" &
    MANAGER_PID=$!
    
    # Wait for manager to start
    log_info "Waiting for manager to initialize..."
    sleep 3
}

start_api() {
    log_info "Starting plfanzen-api with auto-reload..."
    
    cd "$PROJECT_ROOT"
    cargo watch \
        -w crates/api \
        -x "run -p plfanzen-api" \
        --why \
        2>&1 | sed "s/^/${BLUE}[api]${NC} /" &
    API_PID=$!
}

usage() {
    echo "Usage: $0 [options]"
    echo ""
    echo "Starts both plfanzen-manager and plfanzen-api with automatic reloading."
    echo ""
    echo "Options:"
    echo "  --manager-only    Start only the manager service"
    echo "  --api-only        Start only the API service"
    echo "  --no-watch        Run without auto-reload (plain cargo run)"
    echo "  -h, --help        Show this help message"
    echo ""
    echo "Environment variables are loaded from .env file if present."
    echo "Run './scripts/dev-setup.sh' first to set up the development environment."
}

run_without_watch() {
    local service="$1"
    
    cd "$PROJECT_ROOT"
    
    case "$service" in
        manager)
            log_info "Starting plfanzen-manager..."
            cargo run -p plfanzen-manager 2>&1 | sed "s/^/${GREEN}[manager]${NC} /" &
            MANAGER_PID=$!
            ;;
        api)
            log_info "Starting plfanzen-api..."
            cargo run -p plfanzen-api 2>&1 | sed "s/^/${BLUE}[api]${NC} /" &
            API_PID=$!
            ;;
    esac
}

main() {
    local manager_only=false
    local api_only=false
    local no_watch=false
    
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --manager-only)
                manager_only=true
                shift
                ;;
            --api-only)
                api_only=true
                shift
                ;;
            --no-watch)
                no_watch=true
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done
    
    check_dependencies
    load_env
    
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "  ${GREEN}Plfanzen CTF Development Server${NC}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "  Manager:    grpc://localhost:50051"
    echo "  API:        http://localhost:3000"
    echo "  Playground: http://localhost:3000/playground"
    echo "  GraphiQL:   http://localhost:3000/graphiql"
    echo ""
    echo "  Press Ctrl+C to stop all services"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    
    if [ "$api_only" = true ]; then
        if [ "$no_watch" = true ]; then
            run_without_watch api
        else
            start_api
        fi
    elif [ "$manager_only" = true ]; then
        if [ "$no_watch" = true ]; then
            run_without_watch manager
        else
            start_manager
        fi
    else
        if [ "$no_watch" = true ]; then
            run_without_watch manager
            sleep 2
            run_without_watch api
        else
            start_manager
            start_api
        fi
    fi
    
    # Wait for processes
    wait
}

main "$@"
