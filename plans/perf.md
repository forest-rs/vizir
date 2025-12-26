# Performance + SIMD (fearless_simd)

## Goal

Enable fast bulk data operations (domain inference, filtering, binning, aggregation) while:
- keeping correctness and determinism first,
- keeping `vizir_core` small,
- and making optimizations optional and benchmark-driven.

## SIMD policy

- Prefer `fearless_simd` for portable SIMD today (over `std::simd`).
- Keep SIMD behind a feature flag (e.g. `simd`) and provide scalar fallbacks.
- Only introduce SIMD once we have benchmark coverage in a separate crate.

## Targets for SIMD / bulk ops

Likely candidates once we have real columns:
- `min/max` domain inference
- `filter` predicate evaluation over `f64` columns
- `bin` (bucket assignment)
- `aggregate` partial sums/counts

## Staged milestones

### M0: Bench harness (separate crate)

- Add a dedicated benchmark crate (top-level) that depends on the relevant library crates.
- Measure scalar implementations first.

### M1: Column representation

- Ensure the column API can expose contiguous slices when available (or Arrow buffers).
- Provide a fast path when data is contiguous and aligned.

### M2: SIMD fast paths

- Add SIMD implementations using `fearless_simd` where it wins.
- Keep fallbacks and ensure results match scalar versions exactly (tests).

## Open questions

- How do we surface “contiguous slice available” through `TableData` without locking into one backing?
- Do we need specialized kernels per transform, or a small set of reusable kernels?

## Related plans

- `plans/transforms.md` (bulk ops live primarily in transforms)
- `plans/io.md` (Arrow provides good SIMD-friendly buffers)
