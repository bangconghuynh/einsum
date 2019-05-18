use crate::{Contraction, SizedContraction};
use ndarray::prelude::*;
use ndarray::LinalgScalar;

use crate::classifiers::*;

pub trait SingletonViewer<A> {
    fn view_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayViewD<'b, A>
    where
        'a: 'b,
        A: Clone + LinalgScalar;
}

pub trait SingletonContractor<A> {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar;
}

pub trait PairContractor<A> {
    fn contract_pair<'a>(
        &self,
        lhs: &'a ArrayViewD<'a, A>,
        rhs: &'a ArrayViewD<'a, A>,
    ) -> ArrayD<A>
    where
        A: Clone + LinalgScalar;
}

#[derive(Clone, Debug)]
struct Identity {}

impl Identity {
    fn new() -> Self {
        Identity {}
    }
}

impl<A> SingletonViewer<A> for Identity {
    fn view_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayViewD<'b, A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        tensor.view()
    }
}

impl<A> SingletonContractor<A> for Identity {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        tensor.to_owned()
    }
}

#[derive(Clone, Debug)]
struct Permutation {
    permutation: Vec<usize>,
}

impl Permutation {
    fn new(sc: &SizedContraction) -> Self {
        let SizedContraction {
            contraction:
                Contraction {
                    ref operand_indices,
                    ref output_indices,
                    ..
                },
            ..
        } = sc;

        assert_eq!(operand_indices.len(), 1);
        assert_eq!(operand_indices[0].len(), output_indices.len());

        let mut permutation = Vec::new();
        for &c in output_indices.iter() {
            permutation.push(operand_indices[0].iter().position(|&x| x == c).unwrap());
        }

        Permutation { permutation }
    }

    fn from_indices(permutation: &[usize]) -> Self {
        Permutation {
            permutation: permutation.to_vec(),
        }
    }
}

impl<A> SingletonViewer<A> for Permutation {
    fn view_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayViewD<'b, A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        tensor.view().permuted_axes(IxDyn(&self.permutation))
    }
}

impl<A> SingletonContractor<A> for Permutation {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        tensor
            .view()
            .permuted_axes(IxDyn(&self.permutation))
            .to_owned()
    }
}

#[derive(Clone, Debug)]
struct Summation {
    orig_axis_list: Vec<usize>,
    adjusted_axis_list: Vec<usize>,
}

impl Summation {
    fn new(start_index: usize, num_summed_axes: usize) -> Self {
        assert!(num_summed_axes >= 1);
        let orig_axis_list = (start_index..(start_index + num_summed_axes)).collect();
        let adjusted_axis_list = (0..num_summed_axes).map(|_| start_index).collect();

        Summation {
            orig_axis_list,
            adjusted_axis_list,
        }
    }
}

impl<A> SingletonContractor<A> for Summation {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        let mut result = tensor.sum_axis(Axis(self.adjusted_axis_list[0]));
        for &axis in self.adjusted_axis_list[1..].iter() {
            result = result.sum_axis(Axis(axis));
        }
        result
    }
}

#[derive(Clone, Debug)]
struct Diagonalization {
    input_to_output_mapping: Vec<usize>,
    output_shape: Vec<usize>,
}

impl Diagonalization {
    fn new(sc: &SizedContraction) -> Self {
        let SizedContraction {
            contraction:
                Contraction {
                    ref operand_indices,
                    ref output_indices,
                    ..
                },
            ref output_size,
        } = sc;
        assert_eq!(operand_indices.len(), 1);

        let output_shape = output_indices.iter().map(|c| output_size[c]).collect();
        let input_to_output_mapping = operand_indices[0]
            .iter()
            .map(|&c| output_indices.iter().position(|&x| x == c).unwrap())
            .collect();

        Diagonalization {
            input_to_output_mapping,
            output_shape,
        }
    }
}

impl<A> SingletonViewer<A> for Diagonalization {
    fn view_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayViewD<'b, A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        // Construct the stride array on the fly by enumerating (idx, stride) from strides() and
        // adding stride to self.which_index_is_this
        let mut strides = vec![0; self.output_shape.len()];
        for (idx, &stride) in tensor.strides().iter().enumerate() {
            assert!(stride > 0);
            strides[self.input_to_output_mapping[idx]] += stride as usize;
        }

        // Output shape we want is already stored in self.output_shape
        // let t = ArrayView::from_shape(IxDyn(&[3]).strides(IxDyn(&[4])), &sl).unwrap();
        let data_slice = tensor.as_slice_memory_order().unwrap();
        ArrayView::from_shape(
            IxDyn(&self.output_shape).strides(IxDyn(&strides)),
            &data_slice,
        )
        .unwrap()
    }
}

impl<A> SingletonContractor<A> for Diagonalization {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        // We're only using this method if the tensor is not contiguous
        // Clones twice as a result
        let cloned_tensor: ArrayD<A> =
            Array::from_shape_vec(tensor.raw_dim(), tensor.iter().cloned().collect()).unwrap();
        self.view_singleton(&cloned_tensor.view()).into_owned()
    }
}

struct ViewAndSummation<'t, A> {
    view: Box<dyn SingletonViewer<A> + 't>,
    summation: Summation,
}

impl<'t, A> ViewAndSummation<'t, A> {
    fn new(csc: &ClassifiedSingletonContraction) -> Self {
        let mut permutation: Vec<usize> = Vec::new();

        for output_index in csc.output_indices.iter() {
            let input_pos = csc
                .input_indices
                .iter()
                .position(|idx| idx.index == output_index.index)
                .unwrap();
            permutation.push(input_pos);
        }
        for (i, idx) in csc.input_indices.iter().enumerate() {
            if let SingletonIndexInfo::SummedInfo(_) = idx.index_info {
                permutation.push(i);
            }
        }

        let view: Box<dyn SingletonViewer<A>> =
            if permutation == (0..(csc.input_indices.len())).collect::<Vec<usize>>() {
                Box::new(Identity::new())
            } else {
                Box::new(Permutation::from_indices(&permutation))
            };
        let summation = Summation::new(csc.output_indices.len(), csc.summed_indices.len());

        ViewAndSummation { view, summation }
    }
}

impl<'t, A> SingletonContractor<A> for ViewAndSummation<'t, A> {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        let viewed_singleton = self.view.view_singleton(tensor);
        self.summation.contract_singleton(&viewed_singleton)
    }
}

pub struct SingletonContraction<'t, A> {
    op: Box<dyn SingletonContractor<A> + 't>,
}

impl<'t, A: 't> SingletonContraction<'t, A> {
    pub fn new(sc: &SizedContraction) -> Self {
        let csc = ClassifiedSingletonContraction::new(sc);

        if csc.summed_indices.len() == 0 {
            let permutation = Permutation::new(sc);
            SingletonContraction {
                op: Box::new(permutation),
            }
        } else {
            let view_and_summation = ViewAndSummation::new(&csc);
            SingletonContraction {
                op: Box::new(view_and_summation),
            }
        }
    }
}

impl<'t, A> SingletonContractor<A> for SingletonContraction<'t, A> {
    fn contract_singleton<'a, 'b>(&self, tensor: &'b ArrayViewD<'a, A>) -> ArrayD<A>
    where
        'a: 'b,
        A: Clone + LinalgScalar,
    {
        self.op.contract_singleton(tensor)
    }
}
