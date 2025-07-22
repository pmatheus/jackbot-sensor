#!/bin/bash

# Jackbot Integration Test Execution Script
# This script provides easy execution of integration tests with various options

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
DOCKER_COMPOSE_FILE="docker-compose.test.yml"
TEST_TIMEOUT=600
VERBOSE=false
CLEANUP=true
TEST_CATEGORY="all"
PARALLEL=false

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to show usage
show_usage() {
    cat << EOF
Jackbot Integration Test Runner

Usage: $0 [OPTIONS]

Options:
    -h, --help              Show this help message
    -v, --verbose           Enable verbose output
    -t, --timeout SECONDS   Set test timeout (default: 600)
    -c, --category CATEGORY Test category to run (default: all)
                           Options: all, market-data, order-execution, performance, functional
    -n, --no-cleanup        Skip cleanup after tests
    -p, --parallel          Run tests in parallel where possible
    -d, --docker-only       Use Docker environment only
    -l, --local-only        Use local services only (no Docker)
    --build                 Force rebuild of Docker images
    --logs                  Show service logs after test completion

Test Categories:
    all                     Run all integration tests
    market-data            Test market data flow only
    order-execution        Test order execution flow only
    performance            Test performance validation only
    functional             Test functional integration only
    environment            Test environment validation only
    benchmark              Run baseline performance benchmark

Examples:
    $0                                          # Run all tests with Docker
    $0 -c performance -v                       # Run performance tests with verbose output
    $0 -c market-data --no-cleanup            # Run market data tests, keep environment
    $0 --local-only -t 300                    # Run with local services, 5min timeout
    $0 --build -c functional                  # Rebuild and run functional tests

Environment Variables:
    TEST_SENSOR_ENDPOINT          Sensor service endpoint
    TEST_BACKEND_ENDPOINT         Backend service endpoint  
    TEST_TERMINAL_ENDPOINT        Terminal WebSocket endpoint
    TEST_KAFKA_BROKERS           Kafka broker list
    TEST_DATABASE_URL            PostgreSQL connection string
    TEST_REDIS_URL               Redis connection string
    TEST_TIMEOUT_SECONDS         Global test timeout
    RUST_LOG                     Logging level (debug, info, warn, error)

EOF
}

# Function to check prerequisites
check_prerequisites() {
    print_status "Checking prerequisites..."
    
    # Check for required tools
    if ! command -v cargo &> /dev/null; then
        print_error "cargo is required but not installed"
        exit 1
    fi
    
    if [[ "$USE_DOCKER" == "true" ]]; then
        if ! command -v docker &> /dev/null; then
            print_error "docker is required but not installed"
            exit 1
        fi
        
        if ! command -v docker-compose &> /dev/null; then
            print_error "docker-compose is required but not installed"
            exit 1
        fi
        
        # Check if Docker is running
        if ! docker info &> /dev/null; then
            print_error "Docker is not running"
            exit 1
        fi
    fi
    
    print_success "Prerequisites check passed"
}

# Function to setup test environment
setup_environment() {
    print_status "Setting up test environment..."
    
    if [[ "$USE_DOCKER" == "true" ]]; then
        setup_docker_environment
    else
        setup_local_environment
    fi
}

# Function to setup Docker environment
setup_docker_environment() {
    print_status "Starting Docker test environment..."
    
    # Build images if requested
    if [[ "$BUILD_IMAGES" == "true" ]]; then
        print_status "Building Docker images..."
        docker-compose -f "$DOCKER_COMPOSE_FILE" build
    fi
    
    # Start services
    print_status "Starting test services..."
    docker-compose -f "$DOCKER_COMPOSE_FILE" up -d \
        zookeeper kafka postgres-test redis-test mock-exchange \
        graphql-mock websocket-mock prometheus-test grafana-test
    
    # Wait for services to be ready
    print_status "Waiting for services to be ready..."
    
    local max_wait=120
    local wait_time=0
    
    while [[ $wait_time -lt $max_wait ]]; do
        if docker-compose -f "$DOCKER_COMPOSE_FILE" exec -T kafka kafka-topics --bootstrap-server localhost:9092 --list &> /dev/null; then
            break
        fi
        sleep 5
        wait_time=$((wait_time + 5))
        echo -n "."
    done
    echo
    
    if [[ $wait_time -ge $max_wait ]]; then
        print_error "Services failed to start within ${max_wait} seconds"
        show_service_logs
        exit 1
    fi
    
    print_success "Docker environment ready"
}

# Function to setup local environment
setup_local_environment() {
    print_status "Using local test environment..."
    
    # Set default environment variables if not set
    export TEST_SENSOR_ENDPOINT="${TEST_SENSOR_ENDPOINT:-http://localhost:8081}"
    export TEST_BACKEND_ENDPOINT="${TEST_BACKEND_ENDPOINT:-http://localhost:8080}"
    export TEST_TERMINAL_ENDPOINT="${TEST_TERMINAL_ENDPOINT:-ws://localhost:8082}"
    export TEST_KAFKA_BROKERS="${TEST_KAFKA_BROKERS:-localhost:9092}"
    export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgres://test:test@localhost:5433/jackbot_test}"
    export TEST_REDIS_URL="${TEST_REDIS_URL:-redis://localhost:6380}"
    export TEST_TIMEOUT_SECONDS="${TEST_TIMEOUT_SECONDS:-$TEST_TIMEOUT}"
    
    print_success "Local environment configured"
}

# Function to run tests
run_tests() {
    print_status "Running integration tests (category: $TEST_CATEGORY)..."
    
    # Set logging level
    export RUST_LOG="${RUST_LOG:-info}"
    
    local test_command
    local test_args="--nocapture"
    
    if [[ "$PARALLEL" == "true" ]]; then
        test_args="$test_args --test-threads=4"
    else
        test_args="$test_args --test-threads=1"
    fi
    
    if [[ "$VERBOSE" == "true" ]]; then
        export RUST_LOG="debug"
    fi
    
    # Select test based on category
    case "$TEST_CATEGORY" in
        "all")
            test_command="cargo test --package jackbot-execution --test integration_test run_comprehensive_integration_tests -- $test_args"
            ;;
        "market-data")
            test_command="cargo test --package jackbot-execution --test integration_test test_market_data_flow_only -- $test_args"
            ;;
        "order-execution")
            test_command="cargo test --package jackbot-execution --test integration_test test_order_execution_flow_only -- $test_args"
            ;;
        "performance")
            test_command="cargo test --package jackbot-execution --test integration_test test_performance_validation_only -- $test_args"
            ;;
        "functional")
            test_command="cargo test --package jackbot-execution --test integration_test test_functional_integration_only -- $test_args"
            ;;
        "environment")
            test_command="cargo test --package jackbot-execution --test integration_test test_environment_validation -- $test_args"
            ;;
        "benchmark")
            test_command="cargo test --package jackbot-execution --test integration_test benchmark_baseline_performance -- $test_args"
            ;;
        *)
            print_error "Unknown test category: $TEST_CATEGORY"
            exit 1
            ;;
    esac
    
    print_status "Executing: $test_command"
    
    # Run the test with timeout
    if timeout "$TEST_TIMEOUT" bash -c "$test_command"; then
        print_success "Integration tests completed successfully!"
        return 0
    else
        local exit_code=$?
        if [[ $exit_code -eq 124 ]]; then
            print_error "Tests timed out after $TEST_TIMEOUT seconds"
        else
            print_error "Tests failed with exit code $exit_code"
        fi
        return $exit_code
    fi
}

# Function to show service logs
show_service_logs() {
    if [[ "$USE_DOCKER" == "true" ]]; then
        print_status "Service logs:"
        docker-compose -f "$DOCKER_COMPOSE_FILE" logs --tail=50
    fi
}

# Function to cleanup environment
cleanup_environment() {
    if [[ "$CLEANUP" == "false" ]]; then
        print_warning "Skipping cleanup (--no-cleanup specified)"
        print_status "To manually cleanup: docker-compose -f $DOCKER_COMPOSE_FILE down -v"
        return
    fi
    
    if [[ "$USE_DOCKER" == "true" ]]; then
        print_status "Cleaning up Docker environment..."
        docker-compose -f "$DOCKER_COMPOSE_FILE" down -v
        print_success "Cleanup completed"
    fi
}

# Function to generate test report
generate_report() {
    print_status "Generating test report..."
    
    if [[ -f "integration_test_report.json" ]]; then
        print_status "Test report available: integration_test_report.json"
        
        # Show summary if jq is available
        if command -v jq &> /dev/null; then
            local total_tests=$(jq '. | length' integration_test_report.json)
            local passed_tests=$(jq '[.[] | select(.success == true)] | length' integration_test_report.json)
            local failed_tests=$(jq '[.[] | select(.success == false)] | length' integration_test_report.json)
            
            echo
            print_status "Test Summary:"
            echo "  Total Tests: $total_tests"
            echo "  Passed: $passed_tests"
            echo "  Failed: $failed_tests"
            echo "  Success Rate: $(echo "scale=1; $passed_tests * 100 / $total_tests" | bc -l)%"
            
            if [[ $failed_tests -gt 0 ]]; then
                echo
                print_warning "Failed Tests:"
                jq -r '.[] | select(.success == false) | "  - \(.test_name): \(.error_message // "Unknown error")"' integration_test_report.json
            fi
        fi
    fi
    
    # Show Grafana dashboard link if available
    if [[ "$USE_DOCKER" == "true" ]] && docker-compose -f "$DOCKER_COMPOSE_FILE" ps grafana-test | grep -q "Up"; then
        print_status "Grafana dashboard available at: http://localhost:3000 (admin:test)"
    fi
}

# Parse command line arguments
USE_DOCKER=true
BUILD_IMAGES=false
SHOW_LOGS=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -t|--timeout)
            TEST_TIMEOUT="$2"
            shift 2
            ;;
        -c|--category)
            TEST_CATEGORY="$2"
            shift 2
            ;;
        -n|--no-cleanup)
            CLEANUP=false
            shift
            ;;
        -p|--parallel)
            PARALLEL=true
            shift
            ;;
        -d|--docker-only)
            USE_DOCKER=true
            shift
            ;;
        -l|--local-only)
            USE_DOCKER=false
            shift
            ;;
        --build)
            BUILD_IMAGES=true
            shift
            ;;
        --logs)
            SHOW_LOGS=true
            shift
            ;;
        *)
            print_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    print_status "Starting Jackbot Integration Test Suite"
    echo "========================================"
    echo "Category: $TEST_CATEGORY"
    echo "Timeout: ${TEST_TIMEOUT}s"
    echo "Docker: $USE_DOCKER"
    echo "Verbose: $VERBOSE"
    echo "Parallel: $PARALLEL"
    echo "Cleanup: $CLEANUP"
    echo "========================================"
    echo
    
    # Check prerequisites
    check_prerequisites
    
    # Setup environment
    setup_environment
    
    # Run tests
    if run_tests; then
        test_exit_code=0
        print_success "All tests completed successfully!"
    else
        test_exit_code=$?
        print_error "Some tests failed"
    fi
    
    # Show logs if requested
    if [[ "$SHOW_LOGS" == "true" ]]; then
        show_service_logs
    fi
    
    # Generate report
    generate_report
    
    # Cleanup
    cleanup_environment
    
    # Final status
    echo
    if [[ $test_exit_code -eq 0 ]]; then
        print_success "Integration test suite completed successfully! 🎉"
    else
        print_error "Integration test suite completed with failures ❌"
    fi
    
    exit $test_exit_code
}

# Handle script interruption
trap 'print_warning "Test execution interrupted"; cleanup_environment; exit 130' INT TERM

# Run main function
main "$@"