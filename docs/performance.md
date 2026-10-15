# Benchmarking and resource limits

## Rust benchmarks

Run the Divan suites from the repository root:

```sh
cargo bench -p arrpi-core --locked --bench library
cargo bench -p arrpi-core --locked --bench contracts
```

The library suite measures selection and serialization for 100, 1,000 and 10,000 items. Cases cover 20/500-item pages, broad/narrow searches, early/late offsets and lean/heavy native records. Input is constructed outside the measured operation.

The contracts suite measures registry construction, descriptor cloning, complete tool listings and calendar serialization. Use Divan's filters and sample options to isolate a case; `-- --help` lists them.

## Allocation regression tests

```sh
cargo test -p arrpi-core --locked --test library_allocations
```

These tests use an isolated Divan process to check allocation count, peak temporary heap, growing reallocations and growth bytes. They cover movie/TV first pages, narrow searches and late pages over a 10,000-item fixture. The input allocation is excluded. Limits are defined in `crates/arrpi-core/tests/library_allocations.rs`.

Selection retains a bounded top-K page. Late offsets can retain more candidates than first pages; include both when evaluating a change. Check output parity and pagination alongside memory use.

## Runtime profiling

Start the fixture stack and inspect core and UI separately:

```sh
docker compose -p arrpi-test -f deploy/compose.test.yaml up --build -d --wait
docker stats --no-stream arrpi-test-arrpi-1 arrpi-test-arrpi-ui-1
```

For comparisons, use the same native payload, cache policy, CPU/memory limits, request count and concurrency. Measure cold reads, warm cache hits, forced refreshes and concurrent consumers. For UI measurements, load the same routes before sampling. Record image identities and workload settings with results.

RSS includes stacks, mapped pages, allocator retention and caches. Container memory is a separate metric; on cgroup v2, Docker's working set subtracts inactive file pages from charged memory. Summed process RSS can count shared mappings more than once. Temporary parser allocations do not describe total application RAM.

## Enforced bounds

`cache.max_retained_bytes` limits native body backing allocations, including bytes held by consumers after eviction. `cache.max_normalizations` bounds concurrent projections. Resource pools belong to settings revisions; draining requests can retain an older pool after a settings apply. These are not absolute process RSS limits.

Compose applies CPU, memory and PID limits to each service. `deploy/test-containers.sh` verifies runtime contents, image-size budgets, readiness and shutdown behavior. Keep both the standalone core and optional UI within their limits when changing dependencies or build stages. See [configuration](configuration.md#limits-and-outages) for cache and admission settings.
