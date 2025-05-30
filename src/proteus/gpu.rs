use super::sources::{generate_program, DerivedConstants};
use crate::error::{ClError, Error};
use crate::hash_type::HashType;
use crate::poseidon::PoseidonConstants;
use crate::{Arity, BatchHasher, NeptuneField, Strength, DEFAULT_STRENGTH};
use ec_gpu_gen::rust_gpu_tools::{program_closures, Device, Program};
use ff::{Field, PrimeField};
use generic_array::{typenum, ArrayLength, GenericArray};
use std::collections::HashMap;
use std::marker::PhantomData;
use typenum::{U11, U2, U8};

#[cfg(feature = "bls")]
use blstrs::Scalar as Fr;
#[cfg(feature = "pasta")]
use pasta_curves::{Fp, Fq as Fv};

#[cfg(feature = "cuda")]
use ec_gpu_gen::rust_gpu_tools::cuda;
#[cfg(feature = "opencl")]
use ec_gpu_gen::rust_gpu_tools::opencl;
use std::ffi::c_void;

#[derive(Debug)]
enum Buffer<T> {
    #[cfg(feature = "cuda")]
    Cuda(cuda::Buffer<T>),
    #[cfg(feature = "opencl")]
    OpenCl(opencl::Buffer<T>),
}

#[cfg(feature = "cuda")]
impl<T> cuda::KernelArgument for Buffer<T> {
    fn as_c_void(&self) -> *mut c_void {
        match self {
            Self::Cuda(buffer) => buffer.as_c_void(),
            #[cfg(feature = "opencl")]
            Self::OpenCl(_) => unreachable!(),
        }
    }
}

#[cfg(feature = "opencl")]
impl<T> opencl::KernelArgument for Buffer<T> {
    fn push(&self, kernel: &mut opencl::Kernel<'_>) {
        match self {
            #[cfg(feature = "cuda")]
            Self::Cuda(_) => unreachable!(),
            Self::OpenCl(buffer) => buffer.push(kernel),
        };
    }
}

#[derive(Debug)]
struct GpuConstants<F, A>(PoseidonConstants<F, A>)
where
    F: PrimeField,
    A: Arity<F>;

pub struct ClBatchHasher<F, A>
where
    F: NeptuneField,
    A: Arity<F>,
{
    device: Device,
    constants: GpuConstants<F, A>,
    constants_buffer: Buffer<F>,
    max_batch_size: usize,
    program: Program,
}

impl<F, A> GpuConstants<F, A>
where
    F: NeptuneField,
    A: Arity<F>,
{
    fn strength(&self) -> Strength {
        self.0.strength
    }

    fn derived_constants(&self) -> DerivedConstants {
        DerivedConstants::new(self.0.arity(), self.0.full_rounds, self.0.partial_rounds)
    }

    fn to_vec(&self) -> Vec<F> {
        let constants_elements = self.derived_constants().constants_elements;

        let constants = &self.0;
        let mut data = Vec::with_capacity(constants_elements);
        data.push(constants.domain_tag);
        data.extend(&constants.compressed_round_constants);
        data.extend(constants.mds_matrices.m.iter().flatten());
        data.extend(constants.pre_sparse_matrix.iter().flatten());
        for sm in &constants.sparse_matrixes {
            data.extend(&sm.w_hat);
            data.extend(&sm.v_rest);
        }
        data
    }

    /// Returns the name of the kernel that can be be called with those constants
    fn kernel_name(&self) -> String {
        let arity = A::to_usize();
        let strength = self.strength();
        format!("hash_preimages_{}_{}_{}", F::name(), arity, strength)
    }
}

impl<F, A> ClBatchHasher<F, A>
where
    F: NeptuneField,
    A: Arity<F>,
{
    /// Create a new `GPUBatchHasher` and initialize it with state corresponding with its `A`.
    pub(crate) fn new(device: &Device, max_batch_size: usize) -> Result<Self, Error> {
        Self::new_with_strength(device, DEFAULT_STRENGTH, max_batch_size)
    }

    pub(crate) fn new_with_strength(
        device: &Device,
        strength: Strength,
        max_batch_size: usize,
    ) -> Result<Self, Error> {
        let constants = GpuConstants(PoseidonConstants::<F, A>::new_with_strength(strength));
        let program = ec_gpu_gen::program!(device)?;

        // Allocate the buffer only once and re-use it in the hashing steps
        let constants_buffer = match program {
            #[cfg(feature = "cuda")]
            Program::Cuda(ref cuda_program) => cuda_program.run(
                |prog, _| -> Result<Buffer<F>, Error> {
                    let buffer = prog.create_buffer_from_slice(&constants.to_vec())?;
                    Ok(Buffer::Cuda(buffer))
                },
                (),
            )?,
            #[cfg(feature = "opencl")]
            Program::Opencl(ref opencl_program) => opencl_program.run(
                |prog, _| -> Result<Buffer<F>, Error> {
                    let buffer = prog.create_buffer_from_slice(&constants.to_vec())?;
                    Ok(Buffer::OpenCl(buffer))
                },
                (),
            )?,
        };

        Ok(Self {
            device: device.clone(),
            constants,
            constants_buffer,
            max_batch_size,
            program,
        })
    }

    pub(crate) fn device(&self) -> Device {
        self.device.clone()
    }
}

const LOCAL_WORK_SIZE: usize = 256;
impl<F, A> BatchHasher<F, A> for ClBatchHasher<F, A>
where
    F: NeptuneField,
    A: Arity<F>,
{
    fn hash(&mut self, preimages: &[GenericArray<F, A>]) -> Result<Vec<F>, Error> {
        use std::any::TypeId;

        let local_work_size = LOCAL_WORK_SIZE;
        let max_batch_size = self.max_batch_size;
        let batch_size = preimages.len();
        assert!(batch_size <= max_batch_size);

        let global_work_size = calc_global_work_size(batch_size, local_work_size);
        let num_hashes = preimages.len();

        let kernel_name = self.constants.kernel_name();

        let closures = program_closures!(|program, _args| -> Result<Vec<F>, Error> {
            let kernel = program.create_kernel(&kernel_name, global_work_size, local_work_size)?;
            let preimages_buffer = program.create_buffer_from_slice(preimages)?;
            let result_buffer = unsafe { program.create_buffer::<F>(num_hashes)? };

            kernel
                .arg(&self.constants_buffer)
                .arg(&preimages_buffer)
                .arg(&result_buffer)
                .arg(&(preimages.len() as i32))
                .run()?;

            let mut frs = vec![F::ZERO; num_hashes];
            program.read_into_buffer(&result_buffer, &mut frs)?;
            Ok(frs.to_vec())
        });

        let results = self.program.run(closures, ())?;
        Ok(results)
    }

    fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }
}

/// Set `global_work_size` to the smallest value possible, so that the
/// total number of threads is >= `batch-size`.
fn calc_global_work_size(batch_size: usize, local_work_size: usize) -> usize {
    (batch_size / local_work_size) + usize::from(batch_size % local_work_size != 0)
}

#[cfg(test)]
#[cfg(all(feature = "opencl", not(target_os = "macos")))]
mod test {
    use super::*;
    use crate::poseidon::{Poseidon, SimplePoseidonBatchHasher};
    use blstrs::Scalar as Fr;
    use generic_array::sequence::GenericSequence;
    use rand::SeedableRng;
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test_batch_hash_2() {
        let mut rng = XorShiftRng::from_seed(crate::TEST_SEED);
        let device = *Device::all().first().expect("Cannot get a device");

        // NOTE: `batch_size` is not a multiple of `LOCAL_WORK_SIZE`.
        let batch_size = 1025;

        let mut cl_hasher =
            ClBatchHasher::<Fr, U2>::new_with_strength(device, Strength::Standard, batch_size)
                .unwrap();
        let mut simple_hasher =
            SimplePoseidonBatchHasher::<Fr, U2>::new_with_strength(Strength::Standard, batch_size);

        let preimages = (0..batch_size)
            .map(|_| GenericArray::<Fr, U2>::generate(|_| Fr::random(&mut rng)))
            .collect::<Vec<_>>();

        let cl_hashes = cl_hasher.hash(&preimages).unwrap();
        let expected_hashes: Vec<_> = simple_hasher.hash(&preimages).unwrap();

        dbg!(
            &cl_hashes,
            &expected_hashes,
            &cl_hashes.len(),
            &expected_hashes.len()
        );

        assert_eq!(expected_hashes, cl_hashes);
    }

    #[test]
    fn test_calc_global_work_size() {
        let inputs = vec![
            (10, 1),
            (10, 2),
            (10, 3),
            (10, 4),
            (10, 5),
            (10, 6),
            (10, 7),
            (10, 8),
            (10, 9),
            (10, 10),
            (10, 11),
            (1, 1),
            (1, 4),
            (37, 11),
            (37, 57),
            (32, 4),
            (32, 16),
        ];

        for (batch_size, local_work_size) in inputs {
            let global_work_size = calc_global_work_size(batch_size, local_work_size);
            // Make sure the total number of threads is bigger than the batch size.
            assert!(
                global_work_size * local_work_size >= batch_size,
                "global work size is not greater than or equal to the batch size:: {} * {} is not >= {}",
                global_work_size,
                local_work_size,
                batch_size);
            // Make also sure that it's the minimum value.
            assert!(
                (global_work_size - 1) * local_work_size < batch_size,
                "global work size is not minimal: ({} - 1) * {} is not < {}",
                global_work_size,
                local_work_size,
                batch_size
            );
        }
    }
}
