#!/usr/bin/env bash
# SPDX-FileCopyrightText: Aaron Dewes <aaron@nirvati.de>
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Configuration
CLUSTER_NAME="${CLUSTER_NAME:-plfanzen-dev}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-devpassword}"
POSTGRES_USER="${POSTGRES_USER:-plfanzen}"
POSTGRES_DB="${POSTGRES_DB:-plfanzen}"
REPO_DIR="${REPO_DIR:-/tmp/plfanzen-repo}"
GIT_URL="${GIT_URL:-https://github.com/plfanzen/test-ctf.git}"
GIT_BRANCH="${GIT_BRANCH:-main}"

check_dependencies() {
    log_info "Checking dependencies..."
    
    local missing=()
    
    for cmd in docker kubectl helm kind cargo diesel; do
        if ! command -v "$cmd" &> /dev/null; then
            missing+=("$cmd")
        fi
    done
    
    if [ ${#missing[@]} -ne 0 ]; then
        log_error "Missing dependencies: ${missing[*]}"
        echo ""
        echo "Please install the following:"
        echo "  - Docker: https://docs.docker.com/get-docker/"
        echo "  - kubectl: https://kubernetes.io/docs/tasks/tools/"
        echo "  - Helm: https://helm.sh/docs/intro/install/"
        echo "  - kind: https://kind.sigs.k8s.io/"
        echo "  - Rust/Cargo: https://rustup.rs/"
        echo "  - diesel_cli: cargo install diesel_cli --no-default-features --features postgres"
        exit 1
    fi
    
    log_success "All dependencies found"
}

create_cluster() {
    log_info "Setting up Kubernetes cluster..."
    
    if kind get clusters 2>/dev/null | grep -q "$CLUSTER_NAME"; then
        log_info "Cluster '$CLUSTER_NAME' already exists"
    else
        log_info "Creating kind cluster '$CLUSTER_NAME'..."
        cat <<EOF | kind create cluster --name "$CLUSTER_NAME" --config=-
kind: Cluster
apiVersion: kind.x-k8s.io/v1alpha4
nodes:
- role: control-plane
  extraPortMappings:
  - containerPort: 30080
    hostPort: 80
    protocol: TCP
  - containerPort: 30443
    hostPort: 443
    protocol: TCP
  - containerPort: 30432
    hostPort: 5432
    protocol: TCP
EOF
    fi
    kubectl cluster-info --context "kind-$CLUSTER_NAME"
    
    log_success "Kubernetes cluster is ready"
}

deploy_postgres() {
    log_info "Deploying PostgreSQL..."
    
    kubectl apply -f - <<EOF
---
apiVersion: v1
kind: Namespace
metadata:
  name: plfanzen
---
apiVersion: v1
kind: Secret
metadata:
  name: postgres-secret
  namespace: plfanzen
type: Opaque
stringData:
  POSTGRES_USER: "$POSTGRES_USER"
  POSTGRES_PASSWORD: "$POSTGRES_PASSWORD"
  POSTGRES_DB: "$POSTGRES_DB"
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: postgres-pvc
  namespace: plfanzen
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 1Gi
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: postgres
  namespace: plfanzen
spec:
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
      - name: postgres
        image: postgres:18-alpine
        ports:
        - containerPort: 5432
        envFrom:
        - secretRef:
            name: postgres-secret
        volumeMounts:
        - name: postgres-storage
          mountPath: /var/lib/postgresql/18/docker
        readinessProbe:
          exec:
            command: ["pg_isready", "-U", "$POSTGRES_USER", "-d", "$POSTGRES_DB"]
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: postgres-storage
        persistentVolumeClaim:
          claimName: postgres-pvc
---
apiVersion: v1
kind: Service
metadata:
  name: postgres
  namespace: plfanzen
spec:
  type: NodePort
  ports:
  - port: 5432
    targetPort: 5432
    nodePort: 30432
  selector:
    app: postgres
EOF
    
    log_info "Waiting for PostgreSQL to be ready..."
    kubectl wait --for=condition=available --timeout=120s deployment/postgres -n plfanzen
    
    # Give postgres a moment to fully initialize
    sleep 5
    
    log_success "PostgreSQL is running"
}

deploy_traefik() {
    log_info "Deploying Traefik with Helm..."
    
    # Add Traefik Helm repository
    log_info "Adding Traefik Helm repository..."
    helm repo add traefik https://traefik.github.io/charts 2>/dev/null || true
    
    # Update Helm repositories
    log_info "Updating Helm repositories..."
    helm repo update
    
    # Check if Traefik is already installed
    if helm list -n traefik 2>/dev/null | grep -q "traefik"; then
        log_info "Traefik is already installed, skipping..."
    else
        # Create namespace for Traefik
        kubectl create namespace traefik --dry-run=client -o yaml | kubectl apply -f -
        
        # Install Traefik with Helm using NodePort
        log_info "Installing Traefik chart..."
        helm install traefik traefik/traefik \
            --namespace traefik \
            --set service.type=NodePort \
            --set ports.web.nodePort=30080 \
            --set ports.websecure.nodePort=30443 \
            --wait
    fi
    
    log_info "Waiting for Traefik to be ready..."
    kubectl wait --for=condition=available --timeout=120s deployment/traefik -n traefik
    
    log_success "Traefik is running"
}

run_migrations() {
    log_info "Running database migrations..."
    
    cd "$PROJECT_ROOT/crates/api"
    
    export DATABASE_URL="postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}"
    
    # Wait for postgres to accept connections
    local retries=30
    while ! diesel database setup 2>/dev/null && [ $retries -gt 0 ]; do
        log_info "Waiting for database connection... ($retries attempts left)"
        sleep 2
        ((retries--))
    done
    
    if [ $retries -eq 0 ]; then
        log_error "Could not connect to database"
        exit 1
    fi
    
    diesel migration run
    
    log_success "Database migrations completed"
}

setup_repo_dir() {
    log_info "Setting up repository directory..."
    
    mkdir -p "$REPO_DIR"
    
    log_success "Repository directory created at $REPO_DIR"
}

print_env_vars() {
    echo ""
    log_success "Development environment is ready!"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Environment variables for running the services:"
    echo ""
    echo -e "${YELLOW}# For plfanzen-api:${NC}"
    echo "export DATABASE_URL=\"postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}\""
    echo "export MANAGER_ENDPOINT=\"http://localhost:50051\""
    echo ""
    echo -e "${YELLOW}# For plfanzen-manager:${NC}"
    echo "export REPO_DIR=\"$REPO_DIR\""
    echo "export GIT_URL=\"$GIT_URL\""
    echo "export GIT_BRANCH=\"$GIT_BRANCH\""
    echo ""
    echo -e "${YELLOW}# Optional email configuration (for user approval):${NC}"
    echo "# export EMAIL_SMTP_SERVER=\"smtp.example.com\""
    echo "# export EMAIL_SMTP_USERNAME=\"user@example.com\""
    echo "# export EMAIL_SMTP_PASSWORD=\"password\""
    echo "# export EMAIL_FROM_ADDRESS=\"noreply@example.com\""
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "To start the services, run in separate terminals:"
    echo ""
    echo -e "  ${GREEN}Terminal 1 (Manager):${NC}"
    echo "    cd $PROJECT_ROOT && cargo run -p plfanzen-manager"
    echo ""
    echo -e "  ${GREEN}Terminal 2 (API):${NC}"
    echo "    cd $PROJECT_ROOT && cargo run -p plfanzen-api"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Services:"
    echo "  GraphQL Playground: http://localhost:3000/playground"
    echo "  GraphiQL:           http://localhost:3000/graphiql"
    echo ""
    echo "  HTTP (Traefik):     http://localhost:80"
    echo "  HTTPS (Traefik):    https://localhost:443"
    echo ""
}

generate_env_file() {
    local env_file="$PROJECT_ROOT/.env"
    
    cat > "$env_file" <<EOF
# Plfanzen CTF Development Environment
# Generated by dev-setup.sh

# Database
DATABASE_URL=postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@localhost:5432/${POSTGRES_DB}

# Manager service
MANAGER_ENDPOINT=http://localhost:50051
REPO_DIR=$REPO_DIR
GIT_URL=$GIT_URL
GIT_BRANCH=$GIT_BRANCH

# Optional: Email configuration
# EMAIL_SMTP_SERVER=smtp.example.com
# EMAIL_SMTP_USERNAME=user@example.com
# EMAIL_SMTP_PASSWORD=password
# EMAIL_FROM_ADDRESS=noreply@example.com
EOF
    
    log_success "Generated .env file at $env_file"
}

cleanup() {
    log_info "Cleaning up development environment..."
    
    kind delete cluster --name "$CLUSTER_NAME" 2>/dev/null || true
    
    log_success "Cleanup complete"
}

usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  setup     Set up the complete development environment (default)"
    echo "  cleanup   Remove the development Kubernetes cluster"
    echo "  status    Show status of the development environment"
    echo "  help      Show this help message"
    echo ""
    echo "Environment variables:"
    echo "  CLUSTER_NAME       Name of the K8s cluster (default: plfanzen-dev)"
    echo "  POSTGRES_PASSWORD  PostgreSQL password (default: devpassword)"
    echo "  POSTGRES_USER      PostgreSQL user (default: plfanzen)"
    echo "  POSTGRES_DB        PostgreSQL database (default: plfanzen)"
    echo "  REPO_DIR           Repository directory (default: /tmp/plfanzen-repo)"
    echo "  GIT_URL            Git repository URL for challenges"
    echo "  GIT_BRANCH         Git branch (default: main)"
}

status() {
    log_info "Checking development environment status..."
    echo ""
    
    # Check cluster
    if kind get clusters 2>/dev/null | grep -q "$CLUSTER_NAME"; then
        log_success "Kubernetes cluster '$CLUSTER_NAME' is running"
    else
        log_warn "Kubernetes cluster '$CLUSTER_NAME' is not running"
    fi
    
    # Check Traefik
    if helm list -n traefik 2>/dev/null | grep -q "traefik"; then
        if kubectl get deployment traefik -n traefik &>/dev/null; then
            local ready=$(kubectl get deployment traefik -n traefik -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")
            if [ "$ready" == "1" ]; then
                log_success "Traefik is running and ready (Helm, NodePort)"
            else
                log_warn "Traefik deployment exists but is not ready"
            fi
        else
            log_warn "Traefik is not deployed"
        fi
    else
        log_warn "Traefik Helm release not found"
    fi
    
    # Check PostgreSQL
    if kubectl get deployment postgres -n plfanzen &>/dev/null; then
        local ready=$(kubectl get deployment postgres -n plfanzen -o jsonpath='{.status.readyReplicas}' 2>/dev/null || echo "0")
        if [ "$ready" == "1" ]; then
            log_success "PostgreSQL is running and ready"
        else
            log_warn "PostgreSQL deployment exists but is not ready"
        fi
    else
        log_warn "PostgreSQL is not deployed"
    fi
    
    echo ""
}

main() {
    local command="${1:-setup}"
    
    case "$command" in
        setup)
            check_dependencies
            create_cluster
            deploy_traefik
            deploy_postgres
            run_migrations
            setup_repo_dir
            generate_env_file
            print_env_vars
            ;;
        cleanup)
            check_dependencies
            cleanup
            ;;
        status)
            check_dependencies
            status
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            log_error "Unknown command: $command"
            usage
            exit 1
            ;;
    esac
}

main "$@"
