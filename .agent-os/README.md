# Jackbot AgentOS Documentation

This directory contains the three-layer documentation system for the Jackbot crypto trading platform, following the AgentOS methodology.

## Directory Structure

```
.agent-os/
├── layer-1-standards/      # Global coding standards and best practices
├── layer-2-product/        # Product vision and architectural decisions
├── layer-3-specs/          # Detailed technical specifications (SRDs)
└── README.md              # This file
```

## Layer Overview

### Layer 1 - Standards (Global Rules)
Universal coding standards that apply across all Jackbot projects:
- **STANDARDS-rust-development.md**: Rust coding standards, error handling, async patterns
- **STANDARDS-typescript-react.md**: TypeScript/React standards, component patterns, testing

### Layer 2 - Product (Mission & Architecture)
Product-level documentation that bridges business and technical requirements:
- **PRODUCT-jackbot-zero-error-initiative.md**: The 12-hour sprint to achieve zero errors

### Layer 3 - Specs (Implementation Details)
Detailed technical specifications for the zero-error initiative:
- **SRD-001-sensor-critical-fixes.md**: Blocking sensor compilation errors (Hour 0-1)
- **SRD-002-backend-service-fixes.md**: Backend service corrections (Hours 1-3)
- **SRD-003-frontend-optimization.md**: Frontend fixes and performance (Hours 1-7)
- **SRD-004-integration-testing.md**: End-to-end testing strategy (Hours 3-5)
- **SRD-005-deployment-quality-gates.md**: Production deployment (Hours 9-12)

## Quick Start for Developers

### If you're fixing sensor errors:
1. Read **SRD-001-sensor-critical-fixes.md**
2. Follow the Rust standards in **STANDARDS-rust-development.md**
3. Run tests as specified in the SRD

### If you're working on backend:
1. Wait for sensor fixes to complete
2. Read **SRD-002-backend-service-fixes.md**
3. Focus on Arrow version alignment first

### If you're on frontend:
1. Start immediately with **SRD-003-frontend-optimization.md**
2. Follow TypeScript standards in **STANDARDS-typescript-react.md**
3. Can work in parallel with backend team

## Current Sprint Status

**Goal**: Zero errors in 12 hours
**Started**: 2025-07-26
**Deadline**: 12 hours from start

### Timeline
- **Hour 0-1**: Sensor fixes (BLOCKER) ⏳
- **Hour 1-3**: Backend fixes
- **Hour 3-5**: Integration testing
- **Hour 5-7**: Performance optimization  
- **Hour 7-9**: Final fixes
- **Hour 9-12**: Deployment

### Error Count
- **Sensor**: 5 errors (blocking everything)
- **Backend**: ~30 errors across 4 services
- **Frontend**: 35 errors + 218 warnings
- **Total**: 253 issues to resolve

## Key Technical Decisions

1. **Arrow Version**: Standardizing on 51.0.0 across all services
2. **MarketDataInstrument**: Extended with `name_exchange` and `kind` fields
3. **WebSocket Strategy**: Batching messages every 16ms for 60fps updates
4. **Deployment**: Blue-green with automatic rollback on >5% error rate

## Success Criteria

✅ When all these are true:
- Zero compilation errors
- All tests passing
- P99 latency < 100ms
- Error rate < 0.01%
- 60 minutes of stable production operation

## Communication

- **Slack**: #zero-error-initiative
- **Updates**: Every 2 hours
- **Blockers**: Immediate escalation
- **Questions**: Tag @tech-lead

## Version Control

All documentation changes must be committed with clear messages:
```bash
git add .agent-os/
git commit -m "docs: Update SRD-001 with validator fix details"
```

Remember: These are living documents. Update them as the code evolves!

---

*"Every error fixed is a step toward flawless execution in crypto trading."*