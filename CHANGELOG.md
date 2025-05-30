# Changelog

All notable changes to neptune will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://book.async.rs/overview/stability-guarantees.html).

## Unreleased

## 10.0.0

- fix: PoseidonConstants serde Implementation (https://github.com/argumentcomputer/neptune/pull/184)
- chore: make version part of the release commit (https://github.com/argumentcomputer/neptune/pull/185)

## 9.0.0

- refactor: Bump ff & group to 0.13, adjust dependent crates (https://github.com/argumentcomputer/neptune/pull/179)
- ci: Add licenses audits with 'cargo-deny' (https://github.com/argumentcomputer/neptune/pull/181)
- doc: covering PoseidonConstants and Poseidon abstractions with proper docs (https://github.com/argumentcomputer/neptune/pull/178)

## 8.1.1

- chore: update ec-gpu, update const fns (https://github.com/argumentcomputer/neptune/pull/175)
- chore: minor upgrade (https://github.com/argumentcomputer/neptune/pull/173)
- fix: simplify the implementation of arity (https://github.com/argumentcomputer/neptune/pull/172)
- fix: Ensure preimage buffer is padded with newly variables that are actually constrained to be equal to zero (https://github.com/argumentcomputer/neptune/pull/169)

## 8.1.0
- Implement serde for PoseidonConstants (https://github.com/argumentcomputer/neptune/pull/165)

## 8.0.0
- fix: update to newer ec-gpu version (https://github.com/argumentcomputer/neptune/pull/164)
- chore: fix CI MacOS build (https://github.com/argumentcomputer/neptune/pull/161)

## 7.2.0 - 2022-8-9
- Change write_rate_element to add_rate_element to agree with spec.

## 7.1.0 - 2022-8-8

- Add Sponge API to README. (https://github.com/argumentcomputer/neptune/pull/158)
- IO pattern (https://github.com/argumentcomputer/neptune/pull/157)
- Sponge absorb add (https://github.com/argumentcomputer/neptune/pull/156)
- Add sponge circuit synthesis test and remove make_elt method. (https://github.com/argumentcomputer/neptune/pull/154)

## 7.0.0 - 2022-7-21
- Implement sponge construction. (https://github.com/argumentcomputer/neptune/pull/151)
- feat: support other fields (https://github.com/argumentcomputer/neptune/pull/135)
- feat: update dependencies to the latest (https://github.com/argumentcomputer/neptune/pull/150)

## 6.2.0 - 2022-6-10
- Reduce constraints (https://github.com/argumentcomputer/neptune/pull/148)

## 6.1.1 - 2022-5-26
- Implement Arity for U1 (https://github.com/argumentcomputer/neptune/pull/145)

## 6.1.0 - 2022-4-22
- Wasm support (https://github.com/argumentcomputer/neptune/pull/139)

## 6.0.0 - 2022-3-22

- Use bellperson v0.19.0. (https://github.com/argumentcomputer/neptune/pull/141)
- refactor: use field name as part of the function names (https://github.com/argumentcomputer/neptune/pull/136)
- chore: update to Rust 1.56.0 (https://github.com/argumentcomputer/neptune/pull/138)
- chore: use Rust 1.51.0 (https://github.com/argumentcomputer/neptune/pull/134)
- Remove neptune-triton (Futhark) support. (https://github.com/argumentcomputer/neptune/pull/132)
- feat: add vanilla Poseidon benchmarks for Pasta scalar field (https://github.com/argumentcomputer/neptune/pull/124)
- Update README. (https://github.com/argumentcomputer/neptune/pull/129)
- fix: fix warning when compiling with OpenCL (https://github.com/argumentcomputer/neptune/pull/123)
- Fixes gaussian elimination for matrices with 0 entries (https://github.com/argumentcomputer/neptune/pull/122)

## 5.1.0 - 2021-10-21
- Cleanup domain tags (https://github.com/argumentcomputer/neptune/pull/121)
- Fix, test, and enable custom domain tags. (https://github.com/argumentcomputer/neptune/pull/116)
- Update bellperson to v0.18.0 (https://github.com/argumentcomputer/neptune/pull/115)
- Fix spec's sparse factorization w vector (https://github.com/argumentcomputer/neptune/pull/114)

## 5.0.0 - 2021-9-30

- Remove pairing requirement for circuits (https://github.com/argumentcomputer/neptune/pull/111)
- Add support for CUDA (https://github.com/argumentcomputer/neptune/pull/109)
- Use correct global work size (https://github.com/argumentcomputer/neptune/pull/108)
- Properly call ec_gpu_gen::common() (https://github.com/argumentcomputer/neptune/pull/110)
- Use upstream group, ff and pairing dependencies (https://github.com/argumentcomputer/neptune/pull/103)
- Remove unnecessary to_vec (https://github.com/argumentcomputer/neptune/pull/107)
- Pass GPU data from a pre-populated vector (https://github.com/argumentcomputer/neptune/pull/106)

## 4.0.0 - 2021-8-2
- Upgrade to latest rust-gpu-tools (https://github.com/argumentcomputer/neptune/pull/91)
- Rename GPU feature to Futhark (https://github.com/argumentcomputer/neptune/pull/100)
- Improve Clippy on CI (https://github.com/argumentcomputer/neptune/pull/92)
- Fix Clippy warnings (https://github.com/argumentcomputer/neptune/pull/98)
- Remove BatcherType (https://github.com/argumentcomputer/neptune/pull/97)
- Remove GPUSelector (https://github.com/argumentcomputer/neptune/pull/96)

## 3.0.0 - 2021-6-1
- Breaking update of `bellperson` to `0.14` and associated dependency upgrades.

## 2.7 - 2021-3-9
- Use bellperson 0.13.

## 2.6 - 2021-1-21
- Pure OpenCL implementation of batch hashing. (https://github.com/argumentcomputer/neptune/pull/78)

## 2.5 [release committed but never published to crates.io, due to authentication glitch]

## 2.4.0 - 2020-11-17

- Customize batch-sizes of gbench through cli args. (https://github.com/argumentcomputer/neptune/pull/50)
- Remove [most] macos conditional code. (https://github.com/argumentcomputer/neptune/pull/72)
- Refactor, moving device selection to rust-gpu-tools. (https://github.com/argumentcomputer/neptune/pull/70)
- Only clear cache when no hashers are active for futhark context. (https://github.com/argumentcomputer/neptune/pull/68)

## 2.2.0 - 2020-11-01

- Update `bellperson` to `0.12.0`
  [67](https://github.com/argumentcomputer/neptune/pull/67)

## 2.1.1 - 2020-10-30

- Fix `GPUBatchHasher` not clearing GPU caches
  [66](https://github.com/argumentcomputer/neptune/pull/66)

## 2.1.0 - 2020-10-29

- Enable `blst` backend.
  [63](https://github.com/argumentcomputer/neptune/pull/63)
- Explicitly reuse FutharkContext in related Batchers.
  [62](https://github.com/argumentcomputer/neptune/pull/62)
- Make GPUSelector accessible from gbench.
  [59](https://github.com/argumentcomputer/neptune/pull/59)
- Create SECURITY.MD.
  [57](https://github.com/argumentcomputer/neptune/pull/57)
- Avoid compiling any OpenCL on macos.
  [56](https://github.com/argumentcomputer/neptune/pull/56)
- Use latest neptune-triton.
  [55](https://github.com/argumentcomputer/neptune/pull/55)

## 2.0.0 - 2020-08-4

- Add support for domain separation tags. In addition to support for new hash functions built on the Poseidon permutation,
  this introduces a breaking change to the DST used for Strengthened Poseidon.
  [43](https://github.com/argumentcomputer/neptune/pull/43).

