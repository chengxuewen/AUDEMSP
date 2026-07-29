# Coding Style

## Immutability (CRITICAL)

ALWAYS create new objects, NEVER mutate existing ones:

```
// Pseudocode
WRONG:  modify(original, field, value) → changes original in-place
CORRECT: update(original, field, value) → returns new copy with change
```

Rationale: Immutable data prevents hidden side effects, makes debugging easier, and enables safe concurrency.

## Core Principles

### KISS (Keep It Simple)

- Prefer the simplest solution that actually works
- Avoid premature optimization
- Optimize for clarity over cleverness

### DRY (Don't Repeat Yourself)

- Extract repeated logic into shared functions or utilities
- Avoid copy-paste implementation drift
- Introduce abstractions when repetition is real, not speculative

### YAGNI (You Aren't Gonna Need It)

- Do not build features or abstractions before they are needed
- Avoid speculative generality
- Start simple, then refactor when the pressure is real

## File Organization

MANY SMALL FILES > FEW LARGE FILES:
- High cohesion, low coupling
- 200-400 lines typical, 800 max
- Extract utilities from large modules
- Organize by feature/domain, not by type

## Error Handling

ALWAYS handle errors comprehensively:
- Handle errors explicitly at every level
- Provide user-friendly error messages in UI-facing code
- Log detailed error context on the server side
- Never silently swallow errors

### Retry, Backoff & Circuit Breaker

For network calls, external services, and transient failures:

**Exponential backoff with jitter** — never retry immediately or at fixed intervals:
- Base delay: 100ms (minimum retry delay)
- Max delay: 30s (cap exponential growth)
- Jitter: ±25% random variance (prevent thundering herd)
- Max retries: 3 for idempotent operations, 1 for non-idempotent

```rust
// Rust pattern: exponential backoff with jitter
use std::time::Duration;
use rand::Rng;

/// Retry a fallible async operation with exponential backoff.
/// Returns the operation result or the last error after max_retries.
async fn retry_with_backoff<F, Fut, T, E>(
    max_retries: u32,
    base_delay: Duration,
    max_delay: Duration,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut rng = rand::rng();
    let mut attempt = 0;

    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(err) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(err);
                }
                let backoff = (base_delay * 2u32.pow(attempt)).min(max_delay);
                let jitter = rng.random_range(-0.25f64..0.25) * backoff.as_secs_f64();
                let sleep = Duration::from_secs_f64(backoff.as_secs_f64() + jitter);
                tokio::time::sleep(sleep).await;
            }
        }
    }
}
```

**Circuit breaker states**:

```text
         ┌──────────┐
         │  CLOSED  │ ──(failures exceed threshold)──▶
         └──────────┘                               ┌──────────┐
              │                                      │   OPEN   │
              │                                      └──────────┘
              │                                           │
              │                                     (timeout expires)
              │                                           │
              │                                      ┌──────────────┐
              └──(success)──────────────────────────▶│  HALF_OPEN   │
                                                     └──────────────┘
```

- **CLOSED**: normal operation, requests pass through
- **OPEN**: all requests fail-fast (return error immediately, no attempt)
- **HALF_OPEN**: limited probe requests allowed; success → CLOSED, failure → OPEN

```rust
// Rust pattern: circuit breaker (minimal)
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct CircuitBreaker {
    failure_count: AtomicU32,
    last_failure: AtomicU64,  // millis since epoch
    threshold: u32,           // failures to trip
    timeout: Duration,        // time in OPEN before HALF_OPEN
}

impl CircuitBreaker {
    fn should_attempt(&self) -> bool {
        if self.failure_count.load(Ordering::Relaxed) < self.threshold {
            return true;  // CLOSED: allow
        }
        // OPEN: check timeout
        let last = self.last_failure.load(Ordering::Relaxed);
        let elapsed = Instant::now().elapsed().as_millis() as u64 - last;
        elapsed > self.timeout.as_millis() as u64  // timeout → HALF_OPEN
    }

    fn record_result(&self, success: bool) {
        if success {
            self.failure_count.store(0, Ordering::Relaxed);
        } else {
            self.failure_count.fetch_add(1, Ordering::Relaxed);
            self.last_failure.store(
                Instant::now().elapsed().as_millis() as u64,
                Ordering::Relaxed,
            );
        }
    }
}
```

- **Library errors**: define typed errors with `thiserror` — `#[error("connection to {addr} failed after {retries} retries")]`
- **Application errors**: use `anyhow::Context` — `.with_context(|| format!("SFU transport connect failed for peer {peer_id}"))`
- **Do NOT retry on**: authentication failures (4xx), validation errors, resource-not-found (404)
- **DO retry on**: connection refused, timeout, 503 Service Unavailable, DNS resolution failures
- **PIT-11 lesson**: build tool failures (`cargo check`, meson) are not transient — do not wrap in retry logic; fix the root cause

## Input Validation

ALWAYS validate at system boundaries:
- Validate all user input before processing
- Use schema-based validation where available
- Fail fast with clear error messages
- Never trust external data (API responses, user input, file content)

## Naming Conventions

- Variables and functions: `camelCase` with descriptive names
- Booleans: prefer `is`, `has`, `should`, or `can` prefixes
- Interfaces, types, and components: `PascalCase`
- Constants: `UPPER_SNAKE_CASE`
- Custom hooks: `camelCase` with a `use` prefix

## Code Smells to Avoid

### Deep Nesting

Prefer early returns over nested conditionals once the logic starts stacking.

### Magic Numbers

Use named constants for meaningful thresholds, delays, and limits.

### Long Functions

Split large functions into focused pieces with clear responsibilities.

## No Hardcoded Values (CRITICAL)

NEVER hardcode:
- Ports, hostnames, IPs, or URLs — use config files, environment variables, or auto-detection
- Secrets (API keys, passwords, tokens) — use env vars or a secret manager
- Paths, timeouts, buffer sizes — use named constants or configuration

Temporary values that *must* be hardcoded for prototyping MUST be marked with a `TODO:` comment explaining what to replace them with.

Any hardcoded value without a `TODO:` marker is a BLOCKER.

## Code Quality Checklist

Before marking work complete:
- [ ] Code is readable and well-named
- [ ] Functions are small (<50 lines)
- [ ] Files are focused (<800 lines)
- [ ] No deep nesting (>4 levels)
- [ ] Proper error handling
- [ ] No hardcoded values (use constants or config)
- [ ] No mutation (immutable patterns used)
