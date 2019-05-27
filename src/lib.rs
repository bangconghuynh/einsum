// Copyright 2019 Jared Samet
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The `ndarray_einsum` crate implements the `einsum` function, originally
//! implemented for numpy by Mark Wiebe and subsequently reimplemented for
//! other tensor libraries such as Tensorflow and PyTorch. `einsum` (short for Einstein summation)
//! implements general multidimensional tensor contraction. Many linear algebra operations
//! and generalizations of those operations can be expressed as special cases of tensor
//! contraction. Examples include matrix multiplication, matrix trace, vector dot product,
//! tensor Hadamard [element-wise] product, axis permutation, outer product, batch
//! matrix multiplication, bilinear transformations, and many more.
use std::collections::HashMap;

use ndarray::prelude::*;
use ndarray::{Data, IxDyn, LinalgScalar};

mod validation;
pub use validation::{
    einsum_path, validate, validate_and_optimize_order, validate_and_size, Contraction,
    SizedContraction,
};

mod optimizers;
pub use optimizers::{generate_optimized_order, ContractionOrder, OptimizationMethod};

mod contractors;
pub use contractors::{EinsumPath, EinsumPathSteps};
use contractors::{PairContractor, TensordotGeneral};

/// This trait is implemented for all `ArrayBase` variants and is parameterized by the data type.
///
/// It's here so `einsum` and the other functions accepting a list of operands
/// can take a slice `&[&dyn ArrayLike<A>]` where the elements of the slice can have
/// different numbers of dimensions and can be a mixture of `Array` and `ArrayView`.
pub trait ArrayLike<A> {
    fn into_dyn_view(&self) -> ArrayView<A, IxDyn>;
}

impl<A, S, D> ArrayLike<A> for ArrayBase<S, D>
where
    S: Data<Elem = A>,
    D: Dimension,
{
    fn into_dyn_view(&self) -> ArrayView<A, IxDyn> {
        self.view().into_dyn()
    }
}

/// Wrapper around [SizedContraction::contract_operands](struct.SizedContraction.html#method.contract_operands).
pub fn einsum_sc<A: LinalgScalar>(
    sized_contraction: &SizedContraction,
    operands: &[&ArrayLike<A>],
) -> ArrayD<A> {
    sized_contraction.contract_operands(operands)
}

/// Performs all steps of the process in one function: parse the string, compile the execution plan, and execute the contraction.
pub fn einsum<A: LinalgScalar>(
    input_string: &str,
    operands: &[&dyn ArrayLike<A>],
) -> Result<ArrayD<A>, &'static str> {
    let sized_contraction = validate_and_size(input_string, operands)?;
    Ok(einsum_sc(&sized_contraction, operands))
}

/// Compute tensor dot product between two tensors.
///
/// Similar to [the numpy function of the same name](https://docs.scipy.org/doc/numpy/reference/generated/numpy.tensordot.html).
/// Easiest to explain by showing the `einsum` equivalents:
///
/// ```
/// # use ndarray::prelude::*;
/// # use ndarray_einsum_beta::*;
/// let m1 = Array::range(0., (3*4*5*6) as f64, 1.)
///             .into_shape((3,4,5,6,))
///             .unwrap();
/// let m2 = Array::range(0., (4*5*6*7) as f64, 1.)
///             .into_shape((4,5,6,7))
///             .unwrap();
/// assert_eq!(
///     einsum(
///         "ijkl,jklm->im",
///         &[&m1, &m2]
///     ).unwrap(),
///     tensordot(
///         &m1,
///         &m2,
///         &[Axis(1), Axis(2), Axis(3)],
///         &[Axis(0), Axis(1), Axis(2)]
///     )
/// );
///
/// assert_eq!(
///     einsum(
///         "abic,dief->abcdef",
///         &[&m1, &m2]
///     ).unwrap(),
///     tensordot(
///         &m1,
///         &m2,
///         &[Axis(2)],
///         &[Axis(1)]
///     )
/// );
/// ```
pub fn tensordot<A, S, S2, D, E>(
    lhs: &ArrayBase<S, D>,
    rhs: &ArrayBase<S2, E>,
    lhs_axes: &[Axis],
    rhs_axes: &[Axis],
) -> ArrayD<A>
where
    A: ndarray::LinalgScalar,
    S: Data<Elem = A>,
    S2: Data<Elem = A>,
    D: Dimension,
    E: Dimension,
{
    assert_eq!(lhs_axes.len(), rhs_axes.len());
    let lhs_axes_copy: Vec<_> = lhs_axes.iter().map(|x| x.index()).collect();
    let rhs_axes_copy: Vec<_> = rhs_axes.iter().map(|x| x.index()).collect();
    let output_order: Vec<usize> = (0..(lhs.ndim() + rhs.ndim() - 2 * (lhs_axes.len()))).collect();
    let tensordotter = TensordotGeneral::from_shapes_and_axis_numbers(
        &lhs.shape(),
        &rhs.shape(),
        &lhs_axes_copy,
        &rhs_axes_copy,
        &output_order,
    );
    tensordotter.contract_pair(&lhs.view().into_dyn(), &rhs.view().into_dyn())
}

mod slow_versions;
