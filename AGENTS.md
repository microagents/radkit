# radkit Rust Engineering Playbook

This playbook documents the expectations for production-grade Rust within the radkit agent SDK. It covers design principles, ownership discipline, safety, validation, and the workflow needed to ship confidently across native and WASM targets.

## Engineering Values
- Prefer clarity over cleverness. Readability, predictability, and explicit invariants take precedence over micro-optimizations.
- Embrace soundness. Design APIs so the compiler can enforce correctness with types, lifetimes, and traits.
- Minimize hidden state. Favor pure functions, explicit data flow, and narrow interfaces between modules.
- Guard portability. Ensure code paths work across Linux, macOS, Windows, and WASM unless a platform-specific feature is explicitly required.

## Project Configuration
- Declare the current `rust-version` (MSRV) in `Cargo.toml` and update it intentionally. All CI jobs must build and test against this MSRV.
- Use Rust 2021 edition unless a later stable edition is required; keep `rust-toolchain.toml` in sync with the workspace toolchain.
- At the crate root enable the strongest baseline lints we can sustain, e.g.
  ```rust
  #![deny(unsafe_code, unreachable_patterns, unused_must_use)]
  #![warn(clippy::all, clippy::pedantic, clippy::nursery)]
  ```
  Document any relaxed lint so reviewers know why it is needed.
- Keep `lib.rs`/`main.rs` dependency graphs under control by isolating optional functionality in feature-gated modules.

## API and Architecture Design
- Model domain concepts explicitly. Use `struct` and `enum` types with `#[non_exhaustive]` only when future-proofing is intentional.
- Prefer `trait`-driven design over type erasure. Implement conversion traits (`From`, `TryFrom`, `Into`) for ergonomic interop.
- Hide implementation detail behind `pub(crate)` modules. Expose the smallest viable public surface and document invariants via `///` comments.
- Keep constructors and builders validating their inputs; use smart constructors to guarantee invariants after instantiation.
- Separate synchronous, asynchronous, and WASM-specific APIs with feature flags or module boundaries to avoid mixing concerns.

## Ownership and Borrowing Discipline
- Default to borrowing (`&T`, `&mut T`) and slices (`&[T]`) in APIs; take ownership only when necessary. Return `&str`/`&[u8]` instead of allocating `String`/`Vec` when data lives long enough.
- Avoid `clone()` on hot paths. Reach for `Cow<'_, T>`, iterators, or reference-counted pointers (`Arc`, `Rc`) when sharing data.
- Design explicit lifetimes when returning references from structs. Prefer `Arc` + `Weak` for shared ownership with drop ordering requirements.
- Work with interior mutability (`Mutex`, `RwLock`, `parking_lot`, `RefCell`) only when borrowing rules cannot model the invariants. Document why interior mutability is required.
- Validate invariants inside `Drop` implementations and avoid surprising side effects; do not block or panic inside `Drop` paths.

## Async and Concurrency
- Restrict asynchronous APIs to types that are `Send + Sync` unless the lack of thread safety is explicitly documented.
- Never block the runtime. Route CPU intensive or blocking I/O work through `spawn_blocking` or dedicated worker threads.
- Propagate cancellation with `futures::select!`, `tokio::select!`, or cooperative checks so async tasks tear down quickly.
- Use `Arc` + `RwLock`/`Mutex` sparingly; prefer message passing (`mpsc`, `broadcast`, `watch`) or atomics for shared state.
- Validate cross-task ordering with tools like `loom` for tricky concurrency primitives; document assumptions about ordering and visibility.
- Ensure WASM code paths stay single-threaded: guard multi-threaded constructs with `#[cfg(not(target_family = "wasm"))]`.

## Error Handling
- Surface typed errors from library boundaries using `thiserror` or manual enums. Provide context with `anyhow::Context` or `eyre::WrapErr` inside leaf functions, but do not leak `anyhow::Error` across crate boundaries.
- Map external errors into our domain-specific variants early so upstream code can match on them predictably.
- Avoid panics except for programmer bugs (`debug_assert!`) or irrecoverable invariants. Audit `unwrap`/`expect` in PRs; justify any remaining usage in comments.
- Use `Result<T, E>` even for internal helpers if failure is possible. Prefer returning `Option<T>` only when absence is the only failure mode.

## Formatting, Lints, and Static Analysis
- Run `cargo fmt --all` before every commit; CI rejects formatting drift.
- Keep `cargo clippy --workspace --all-targets --all-features -D warnings` green. Document every `#[allow]` with a reason and a follow-up issue if needed.
- Periodically run `cargo fix --allow-dirty` to adopt new idioms introduced by compiler upgrades, but always review the diff manually.
- Use `cargo udeps` to remove unused dependencies and `cargo deny` or `cargo audit` to flag vulnerable or unlicensed crates as part of release pre-checks.

## Testing and Validation
- Require unit tests for every public function, trait impl, state machine, and error path. Favor table-driven tests for deterministic coverage.
- Exercise async logic with `tokio::test` (native) and `wasm-bindgen-test` (WASM). For concurrent code, include regression tests that drive cancellation and ordering edge cases.
- Add property or fuzz tests (`proptest`, `arbitrary`, `cargo fuzz`) for parsing, serialization, and protocol logic.
- Keep integration tests hermetic by mocking filesystem, network, and time; use contract tests when integrating with real services is unavoidable.
- Track branch coverage with `cargo llvm-cov` (native) and `wasm-bindgen-test --coverage` (WASM). Treat meaningful coverage gaps as blockers before merge.

## Observability and Diagnostics
- Emit structured logs through `tracing` with consistent target names. Use `#[tracing::instrument]` on async entry points to preserve spans across awaits.
- Attach error context (`tracing::error!`, `tracing::warn!`) with actionable messages; avoid logging secrets or personal data.
- Collect metrics via the workspace metrics facade (histograms for latency, counters for events). Update dashboards or alerts when behavior changes.
- Provide troubleshooting guides in module-level docs for complex components; include sample traces or metric expectations where helpful.

## Performance and Memory
- Favor iterators, slices, and views over temporary allocations. Reuse buffers with `SmallVec`, `Bytes`, or pooling strategies when profiling proves the need.
- Benchmark critical paths with `cargo bench` or `criterion` before landing optimizations. Capture flamegraphs (`cargo flamegraph`) to validate wins.
- Keep hot data structures in cache-friendly shapes (avoid large enums with padding; consider `#[repr(C)]` only with clear justification).
- Audit cloning, locking, and allocations during code review; measure before optimizing and document trade-offs.

## Dependency Management
- Default to `std`, `futures`, and `tokio` (or WASM executor) primitives. Introduce new crates only with an ADR or design note describing rationale, maintenance plan, and licensing.
- Prefer minimal feature sets. Disable default features for crates we import and enable exactly what we need.
- Regularly run `cargo update -p <crate>` alongside release branches; capture upgrade notes in `CHANGELOG.md`.
- Keep native-only dependencies behind `cfg` gates or feature flags. Provide WASM-friendly alternatives to avoid pulling incompatible crates into the WASM artifact.

## Runtime Compatibility Helpers
- Route native/WASM behavioral differences through `radkit/src/compat.rs` so calling sites stay portable; prefer using the helpers (`MaybeSend`, `compat::spawn`, `compat::time`, etc.) instead of ad-hoc `#[cfg]` blocks in business logic.
- Extend `radkit/src/compat.rs` when new cross-target abstractions are needed, mirroring APIs on both sides and documenting any WASM limitations in the module so downstream crates can plan around them.
- Gate platform-specific dependencies inside `compat.rs` and re-export neutral aliases; avoid leaking executor-specific types (e.g., `tokio::Mutex`) outside the module.
- Exercise both native and WASM code paths whenever `compat.rs` changes by running native tests plus the relevant WASM smoke/regression tests to catch divergences early.

## Build Targets
- Maintain dual build targets:
  - Native: `cargo build --workspace --all-features` for development, load testing, and services.
  - WASM: `cargo build --target wasm32-wasi --release` for deployment.
- Guard native-only build steps in `build.rs` with `cfg!(not(target_arch = "wasm32"))` and fail fast with helpful messages when the target is unsupported.
- Run smoke tests for WASM artifacts via `wasmtime`, `wasmer`, or the host embedding to validate ABI changes.

## Release and Packaging
- Follow semantic versioning. Breaking API changes require a major version bump and migration guide under `docs/`.
- Automate tagging with `cargo release` or an equivalent tool; include generated documentation and release notes in the pipeline output.
- Ship reproducible builds: pin toolchain versions, vetted dependencies, and ensure `cargo package` is clean (`cargo package --allow-dirty` is prohibited).
- Archive WASM artifacts with their interface metadata and checksum for downstream verification.

## Code Review and Collaboration
- Open design discussions (RFCs or ADRs) before implementing cross-cutting changes or introducing new dependencies.
- Keep PRs focused and under ~400 lines where possible. Include a narrative explaining the change, validation performed, and follow-up tasks.
- Reviewers check for ownership leaks, blocking operations, error propagation, and missing tests before approving.
- Merge only with green CI covering formatting, clippy, unit/integration tests, WASM tests, and audit checks.

## Workflow Checklist
1. Update or add targeted tests before implementing behavior changes.
2. Run `cargo fmt`, `cargo clippy --all-targets --all-features -D warnings`, and native tests (`cargo test --workspace`).
3. Run WASM tests or smoke checks (`wasm-pack test --node`, `wasmtime <artifact>.wasm`, or equivalent) when code affects shared logic.
4. Inspect coverage via `cargo llvm-cov` (native) and `wasm-bindgen-test --coverage` for relevant crates; backfill missing cases.
5. Verify documentation builds cleanly with `cargo doc --no-deps` and keep public API changes documented.
6. Capture release notes or changelog entries for externally visible changes.
7. Land the change only after peer review and green CI across all required jobs.

Adhering to these practices keeps the radkit SDK reliable, portable, and maintainable while delivering a disciplined developer experience.
