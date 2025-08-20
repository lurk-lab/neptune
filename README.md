# Neptune ![crates.io](https://img.shields.io/crates/v/neptune.svg) [![neptune](https://github.com/argumentcomputer/neptune/actions/workflows/rust.yml/badge.svg)](https://github.com/lurk-lab/neptune/actions/workflows/rust.yml) [![dependency status](https://deps.rs/repo/github/argumentcomputer/neptune/status.svg)](https://deps.rs/repo/github/argumentcomputer/neptune)


## About
Neptune is a Rust implementation of the [Poseidon hash function](https://www.poseidon-hash.info/) tuned for
[Filecoin](https://filecoin.io/).

Neptune has been [audited by ADBK Consulting](poseidon-in-filecoin-final-report.pdf) and deemed fully compliant with the
paper ([Starkad and Poseidon: New Hash Functions for Zero Knowledge Proof
Systems](https://eprint.iacr.org/2019/458.pdf)).

Neptune was initially specialized to the [BLS12-381 curve](https://electriccoin.co/blog/new-snark-curve/). Although the
API allows for type specialization to other fields, the round numbers, constants, and s-box selection may not be
correct. As long as the alternate field is a prime field of ~256 bits, the 128-bit security Neptune targets will apply.
There is a run-time assertion which will fail if constants are generated for a field whose elements do not have a
representation of exactly 32 byte. The [Pasta Curves](https://github.com/zcash/pasta_curves) meet these criteria and are
explicitly supported by Neptune.

At the time of the 1.0.0 release, Neptune on RTX 2080Ti GPU can build 8-ary Merkle trees for 4GiB of input in 16 seconds.

## Implementation Specification

Filecoin's Poseidon specification is published in the Filecoin specification document [here](https://spec.filecoin.io/#section-algorithms.crypto.poseidon). Additionally, Markdown and PDF versions are mirrored in this repo in the [`spec`](spec) directory.

### Contributing to the Spec

### PDF Rendering Instructions

The spec's PDF is rendered using [Typora](https://typora.io/). Download the spec's Markdown file [here](spec/poseidon_spec.md), open the file in Typora, make and save your changes, then export the file as a PDF.

### Ensuring Spec Documents Stay in Sync

When making changes to the spec documents in `neptune`, make sure that the spec's PDF file `poseidon_spec.pdf` is the PDF rendering of the Markdown spec `poseidon_spec.md`.

If you make changes to the spec in `neptune`, you must make those same changes to the Filecoin spec [here](https://github.com/filecoin-project/specs/blob/master/content/algorithms/crypto/poseidon.md), thus ensuring all three document's (one Markdown+Latex and one PDF in `neptune` and one Markdown+MathJax in [`filecoin-project/specs`](https://github.com/filecoin-project/specs/)) stay in sync.

## Environment variables

 - `EC_GPU_FRAMEWORK=<cuda | opencl>` allows to select whether the CUDA or OpenCL implementation should be used. If not set, `cuda` will be used if available.

 - `EC_GPU_CUDA_NVCC_ARGS`

By default the CUDA kernel is compiled for several architectures, which may take a long time. `EC_GPU_CUDA_NVCC_ARGS` can be used to override those arguments. The input and output file will still be automatically set.

    // Example for compiling the kernel for only the Turing architecture
    EC_GPU_CUDA_NVCC_ARGS="--fatbin --gpu-architecture=sm_75 --generate-code=arch=compute_75,code=sm_75"

## Rust feature flags

Neptune also supports batch hashing and tree building, which can be performed on a GPU. GPU batch hashing is implemented in pure CUDA/OpenCL. The pure CUDA/OpenCL batch hashing is provided by the internal `proteus` module. To use `proteus`, compile `neptune` with the `opencl` and/or `cuda` feature.

The `cuda` and `opencl` feature can be used independently or together. If both `cuda` and `opencl` are used, you can also select which implementation to use via the `NEPTUNE_GPU_FRAMEWORK` environment variable.

### Arities

The CUDA/OpenCL kernel (enabled with the `cuda/opencl` feature) is generated with specific arities. Those arities need to be specified at compile-time via Rust feature flags. Available features are `arity2`, `arity4`, `arity8`, `arity11`, `arity16`, `arity24`, `arity36`. When the `strengthened` feature is enables, there will be an additional strengthened version available for each arity.

When using the `cuda` feature, the kernel is generated at compile-time. The more arities are used, the longer is the compile time. Hence, by default there are no specific arities enabled. You need to set at least one yourself.

### Fields

The CUDA/OpenCL kernel (enabled with the `cuda/opencl` feature) is generated for specific fields. Those fields need to be specified at compile-time via Rust feature flags. Available features are `bls` for BLS12-381 and `pasta` for the Pallas and Vesta curves' scalar fields.

## Running the tests

As the compile-time of the kernel depends on how many arities are used, there are no arities enabled by default. In order to run the test, all arities need to explicitly be enabled. To run all tests on e.g. the CUDA implementation, run:

    cargo test --no-default-features --features cuda,bls,pasta,arity2,arity4,arity8,arity11,arity16,arity24,arity36

## Benchmarking Poseidon by Field and Preimage Length

Benchmark Poseidon over the BLS12-381, Pallas, and Vesta scalar fields for preimages of length `2`, `4`, `8`, or `11` using:

    cargo bench arity-<preimage len>

Benchmark Poseidon over a specific field (`bls`, `pallas`, or `vesta`) and preimage length using:

    cargo bench arity-<preimage len>/<field name>

## Sponge API

Neptune implements the [Secure Sponge API for Field Elements](https://hackmd.io/bHgsH6mMStCVibM_wYvb2w) and serves as its reference implementation. The [`SpongeAPI` trait](https://github.com/argumentcomputer/neptune/blob/master/src/sponge/api.rs) defines the relevant API methods. See tests in source for simple examples of API usage [with circuits](https://github.com/argumentcomputer/neptune/blob/master/src/sponge/circuit.rs) and [without circuits](https://github.com/argumentcomputer/neptune/blob/master/src/sponge/vanilla.rs).

## History

Neptune was originally bootstrapped from [Dusk's reference implementation](https://github.com/dusk-network/dusk-poseidon-merkle).

## Changes
[CHANGELOG](CHANGELOG.md)

## License

MIT or Apache 2.0
