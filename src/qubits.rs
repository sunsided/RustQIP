use std::fmt;
use std::rc::Rc;

use num::complex::Complex;

use crate::pipeline::*;
use crate::state_ops::*;
use crate::types::Precision;

/// Possible relations to a parent qubit
pub enum Parent {
    Owned(Vec<Qubit>, Option<StateModifier>),
    Shared(Rc<Qubit>),
}

/// A qubit object, possible representing multiple physical qubit indices.
pub struct Qubit {
    pub indices: Vec<u64>,
    pub parent: Option<Parent>,
    pub id: u64,
}

impl Qubit {
    fn new(id: u64, indices: Vec<u64>) -> Result<Qubit, &'static str> {
        if indices.is_empty() {
            Err("Qubit must have nonzero number of indices.")
        } else {
            Ok(Qubit {
                indices,
                parent: None,
                id,
            })
        }
    }

    /// Create a handle for feeding values.
    pub fn handle(&self) -> QubitHandle {
        QubitHandle{ indices: self.indices.clone() }
    }

    /// Merge qubits to for a new qubit object.
    pub fn merge_with_modifier(id: u64, qubits: Vec<Qubit>, modifier: Option<StateModifier>) -> Qubit {
        let mut all_indices = Vec::new();

        for q in qubits.iter() {
            all_indices.extend(q.indices.iter());
        }
        all_indices.sort();

        Qubit {
            indices: all_indices,
            parent: Some(Parent::Owned(qubits, modifier)),
            id,
        }
    }

    /// Split the relative indices out of `q` into its own qubit, remaining live in second qubit.
    pub fn split(ida: u64, idb: u64, q: Qubit, indices: Vec<u64>) -> Result<(Qubit, Qubit), &'static str> {
        for indx in &indices {
            if *indx > (q.indices.len() as u64) {
                return Err("All indices for splitting must be below q.n");
            }
        }
        if indices.len() == q.indices.len() {
            Err("Indices must leave at least one index.")
        } else if indices.is_empty() {
            Err("Indices must contain at least one index.")
        } else {
            let selected_indices: Vec<u64> = indices.into_iter().map(|i| q.indices[i as usize]).collect();
            Self::split_absolute(ida, idb, q, selected_indices)
        }
    }

    /// Split a qubit in two, with one having the indices in `selected_indices`
    pub fn split_absolute(ida: u64, idb: u64, q: Qubit, selected_indices: Vec<u64>) -> Result<(Qubit, Qubit), &'static str> {
        if selected_indices.len() == q.indices.len() {
            return Err("Cannot split out all indices into own qubit.");
        } else if selected_indices.is_empty() {
            return Err("Must provide indices to split.");
        }
        for indx in &selected_indices {
            if !q.indices.contains(indx) {
                return Err("All indices must exist in qubit to be split.");
            }
        };

        let remaining = q.indices.clone()
            .into_iter()
            .filter(|x| !selected_indices.contains(x))
            .collect();
        let shared_parent = Rc::new(q);

        Ok((Qubit {
            indices: selected_indices,
            parent: Some(Parent::Shared(shared_parent.clone())),
            id: ida,
        }, Qubit {
            indices: remaining,
            parent: Some(Parent::Shared(shared_parent.clone())),
            id: idb,
        }))
    }

    /// Get number of qubits in this Qubit object
    pub fn n(&self) -> u64 {
        self.indices.len() as u64
    }
}

impl std::cmp::Eq for Qubit {}

impl std::cmp::PartialEq for Qubit {
    fn eq(&self, other: &Qubit) -> bool {
        self.id == other.id
    }
}

impl std::cmp::Ord for Qubit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl std::cmp::PartialOrd for Qubit {
    fn partial_cmp(&self, other: &Qubit) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for Qubit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let int_strings = self.indices.iter()
            .map(|x| x.clone().to_string())
            .collect::<Vec<String>>();

        write!(f, "Qubit[{}][{}]", self.id.to_string(), int_strings.join(", "))
    }
}

pub struct QubitHandle {
    indices: Vec<u64>
}

impl QubitHandle {
    pub fn make_init_from_index<P: Precision>(&self, index: u64) -> Result<QubitInitialState<P>, &'static str> {
        if index < 1 << self.indices.len() as u64 {
            Ok((self.indices.clone(), InitialState::Index(index)))
        } else {
            Err("Index too large for QubitHandle")
        }
    }
    pub fn make_init_from_state<P: Precision>(&self, state: Vec<Complex<P>>) -> Result<QubitInitialState<P>, &'static str> {
        if state.len() == 1 << self.indices.len() {
            Ok((self.indices.clone(), InitialState::FullState(state)))
        } else {
            Err("State not correct size for QubitHandle (must be 2^n)")
        }
    }
}

/// A builder which supports non-unitary operations
pub trait NonUnitaryBuilder {
    /// Add a measure op to the pipeline for `q` and return a reference which can
    /// later be used to access the measured value from the results of `pipeline::run`.
    fn measure(&mut self, q: Qubit) -> (Qubit, u64);
}

/// A builder which support unitary operations
pub trait UnitaryBuilder {
    // Things like X, Y, Z, NOT, H, SWAP, ... go here

    /// Build a builder which uses `q` as context.
    fn with_context(&mut self, q: Qubit) -> ConditionalContextBuilder;

    /// Build a generic matrix op, apply to `q`, if `q` is multiple indices and
    /// mat is 2x2, apply to each index, otherwise returns an error if the matrix is not the correct
    /// size for the number of indices in `q` (mat.len() == 2^(2n)).
    fn mat(&mut self, q: Qubit, mat: &[Complex<f64>]) -> Result<Qubit, &'static str>;

    /// Build a matrix op from real numbers, apply to `q`, if `q` is multiple indices and
    /// mat is 2x2, apply to each index, otherwise returns an error if the matrix is not the correct
    /// size for the number of indices in `q` (mat.len() == 2^(2n)).
    fn real_mat(&mut self, q: Qubit, mat: &[f64]) -> Result<Qubit, &'static str> {
        self.mat(q, from_reals(mat).as_slice())
    }

    /// Apply NOT to `q`, if `q` is multiple indices, apply to each
    fn not(&mut self, q: Qubit) -> Qubit {
        self.x(q)
    }

    /// Apply X to `q`, if `q` is multiple indices, apply to each
    fn x(&mut self, q: Qubit) -> Qubit {
        self.real_mat(q, &[0.0, 1.0, 1.0, 0.0]).unwrap()
    }

    /// Apply Y to `q`, if `q` is multiple indices, apply to each
    fn y(&mut self, q: Qubit) -> Qubit {
        self.mat(q, from_tuples(&[(0.0, 0.0), (0.0, -1.0), (0.0, 0.0), (0.0, 1.0)])
            .as_slice()).unwrap()
    }

    /// Apply Z to `q`, if `q` is multiple indices, apply to each
    fn z(&mut self, q: Qubit) -> Qubit {
        self.real_mat(q, &[1.0, 0.0, 0.0, -1.0]).unwrap()
    }

    /// Apply H to `q`, if `q` is multiple indices, apply to each
    fn hadamard(&mut self, q: Qubit) -> Qubit {
        let inv_sqrt = 1.0f64 / 2.0f64.sqrt();
        self.real_mat(q, &[inv_sqrt, inv_sqrt, inv_sqrt, -inv_sqrt]).unwrap()
    }

    /// Apply SWAP to `qa` and `qb`
    fn swap(&mut self, qa: Qubit, qb: Qubit) -> Result<(Qubit, Qubit), &'static str> {
        let op = self.make_swap_op(&qa, &qb)?;
        let qa_indices = qa.indices.clone();
        let q = self.merge_with_op(vec![qa, qb], Some(op));
        self.split_absolute(q, qa_indices)
    }

    /// Make an operation from the boxed function `f`. This maps c|`q_in`>|`q_out`> to
    /// c*e^i`theta`|`q_in`>|`q_out` ^ `indx`> where `indx` and `theta` are the outputs from the
    /// function `f(x) = (indx, theta)`
    fn apply_function(&mut self, q_in: Qubit, q_out: Qubit, f: Box<Fn(u64) -> (u64, f64) + Send + Sync>) -> (Qubit, Qubit);

    /// Merge the qubits in `qs` into a single qubit.
    fn merge(&mut self, qs: Vec<Qubit>) -> Qubit {
        self.merge_with_op(qs, None)
    }

    /// Split the qubit `q` into two qubits, one with relative `indices` and one with the remaining.
    fn split(&mut self, q:Qubit, indices: Vec<u64>) -> Result<(Qubit, Qubit), &'static str> {
        for indx in &indices {
            if *indx > (q.indices.len() as u64) {
                return Err("All indices for splitting must be below q.n");
            }
        }
        if indices.is_empty() {
            Err("Indices must contain at least one index.")
        } else if indices.len() == q.indices.len() {
            Err("Indices must leave at least one index.")
        } else {
            let selected_indices: Vec<u64> = indices.into_iter().map(|i| q.indices[i as usize]).collect();
            self.split_absolute(q, selected_indices)
        }
    }

    /// Split the qubit `q` into two qubits, one with `selected_indices` and one with the remaining.
    fn split_absolute(&mut self, q: Qubit, selected_indices: Vec<u64>) -> Result<(Qubit, Qubit), &'static str>;

    /// Split the qubit into many qubits, each with the given set of indices.
    fn split_absolute_many(&mut self, q: Qubit, index_groups: Vec<Vec<u64>>) -> Result<(Vec<Qubit>, Qubit), &'static str> {
        Ok(index_groups.into_iter().fold((vec![], q), |(mut qs, q), indices| {
            let (hq, tq) = self.split_absolute(q, indices).unwrap();
            qs.push(hq);
            (qs, tq)
        }))
    }

    /// Split `q` into a single qubit for each index.
    fn split_all(&mut self, q: Qubit) -> Vec<Qubit> {
        let mut indices: Vec<Vec<u64>> = q.indices.iter().cloned().map(|i| vec![i]).collect();
        indices.pop();
        // Cannot fail since all indices are from q.
        let (mut qs, q) = self.split_absolute_many(q, indices).unwrap();
        qs.push(q);
        qs
    }

    /// Build a generic matrix op.
    fn make_mat_op(&self, q: &Qubit, data: Vec<Complex<f64>>) -> QubitOp {
        QubitOp::Matrix(q.indices.clone(), data)
    }

    /// Build a swap op. qa and qb must have the same number of indices.
    fn make_swap_op(&self, qa: &Qubit, qb: &Qubit) -> Result<QubitOp, &'static str> {
        if qa.indices.len() == qb.indices.len() {
            Ok(QubitOp::Swap(qa.indices.clone(), qb.indices.clone()))
        } else {
            Err("Swap must be made from two qubits of equal size.")
        }
    }

    /// Make a function op. f must be boxed so that this function doesn't need to be parameterized.
    fn make_function_op(&self, q_in: &Qubit, q_out: &Qubit, f: Box<Fn(u64) -> (u64, f64) + Send + Sync>) -> QubitOp {
        QubitOp::Function(q_in.indices.clone(), q_out.indices.clone(), f)
    }

    /// Merge qubits using a generic state processing function.
    fn merge_with_op(&mut self, qs: Vec<Qubit>, operator: Option<QubitOp>) -> Qubit;

    /// Measure all qubit states and probabilities, does not edit state (thus Unitary). Returns
    /// qubit and handle.
    fn stochastic_measure(&mut self, q: Qubit) -> (Qubit, u64);
}

/// Helper function for Boxing static functions and applying using the given UnitaryBuilder.
pub fn apply_function<F: 'static + Fn(u64) -> (u64, f64) + Send + Sync>(b: &mut UnitaryBuilder, q_in: Qubit, q_out: Qubit, f: F) -> (Qubit, Qubit) {
    b.apply_function(q_in, q_out, Box::new(f))
}

/// A basic builder for unitary and non-unitary ops.
#[derive(Default)]
pub struct OpBuilder {
    qubit_index: u64,
    op_id: u64
}

impl OpBuilder {
    /// Build a new OpBuilder
    pub fn new() -> OpBuilder {
        OpBuilder::default()
    }

    /// Build a new qubit with `n` indices
    pub fn qubit(&mut self, n: u64) -> Result<Qubit, &'static str> {
        if n == 0 {
            Err("Qubit n must be greater than 0.")
        } else {
            let base_index = self.qubit_index;
            self.qubit_index += n;

            Qubit::new(self.get_op_id(), (base_index..self.qubit_index).collect())
        }
    }

    /// Build a new qubit with `n` indices, return it plus a handle which can be
    /// used for feeding in an initial state.
    pub fn qubit_and_handle(&mut self, n: u64) -> Result<(Qubit, QubitHandle), &'static str> {
        let q = self.qubit(n)?;
        let indices = q.indices.clone();
        Ok((q, QubitHandle{ indices }))
    }

    fn get_op_id(&mut self) -> u64 {
        let tmp = self.op_id;
        self.op_id += 1;
        tmp
    }
}

impl NonUnitaryBuilder for OpBuilder {
    fn measure(&mut self, q: Qubit) -> (Qubit, u64) {
        let id = self.get_op_id();
        let modifier = StateModifier::new_measurement(String::from("measure"), id, q.indices.clone());
        let modifier = Some(modifier);
        let q = Qubit::merge_with_modifier(id, vec![q], modifier);
        (q, id)
    }
}

impl UnitaryBuilder for OpBuilder {
    fn with_context(&mut self, q: Qubit) -> ConditionalContextBuilder {
        ConditionalContextBuilder {
            parent_builder: self,
            conditioned_qubit: Some(q),
        }
    }

    fn mat(&mut self, q: Qubit, mat: &[Complex<f64>]) -> Result<Qubit, &'static str> {
        // Special case for broadcasting ops
        if q.indices.len() > 1 && mat.len() == (2 * 2) {
            let qs = self.split_all(q);
            let qs = qs.into_iter().map(|q| self.mat(q, mat).unwrap()).collect();
            Ok(self.merge_with_op(qs, None))
        } else {
            let expected_mat_size = 1 << (2*q.indices.len());
            if expected_mat_size != mat.len() {
                Err("Matrix not of expected size")
            } else {
                let op = self.make_mat_op(&q, mat.to_vec());
                Ok(self.merge_with_op(vec![q], Some(op)))
            }
        }
    }

    fn apply_function(&mut self, q_in: Qubit, q_out: Qubit, f: Box<Fn(u64) -> (u64, f64) + Send + Sync>) -> (Qubit, Qubit) {
        let op = self.make_function_op(&q_in, &q_out, f);
        let in_indices = q_in.indices.clone();
        let q = self.merge_with_op(vec![q_in, q_out], Some(op));
        self.split_absolute(q, in_indices).unwrap()
    }

    fn split_absolute(&mut self, q: Qubit, selected_indices: Vec<u64>) -> Result<(Qubit, Qubit), &'static str> {
        Qubit::split_absolute(self.get_op_id(), self.get_op_id(), q, selected_indices)
    }

    fn merge_with_op(&mut self, qs: Vec<Qubit>, op: Option<QubitOp>) -> Qubit {
        let modifier = op.map(|op|StateModifier::new_unitary(String::from("unitary"), op));
        Qubit::merge_with_modifier(self.get_op_id(), qs, modifier)
    }

    fn stochastic_measure(&mut self, q: Qubit) -> (Qubit, u64) {
        let id = self.get_op_id();
        let modifier = StateModifier::new_stochastic_measurement(String::from("stochastic"), id, q.indices.clone());
        let modifier = Some(modifier);
        let q = Qubit::merge_with_modifier(id, vec![q], modifier);
        (q, id)
    }
}

/// An op builder which depends on the value of a given qubit (COPs)
pub struct ConditionalContextBuilder<'a> {
    parent_builder: &'a mut UnitaryBuilder,
    conditioned_qubit: Option<Qubit>,
}

impl<'a> ConditionalContextBuilder<'a> {
    /// Release the qubit used to build this builder
    pub fn release_qubit(self: Self) -> Qubit {
        match self.conditioned_qubit {
            Some(q) => q,
            None => panic!("Conditional context builder failed to populate qubit.")
        }
    }

    fn get_conditional_qubit(&mut self) -> Qubit {
        self.conditioned_qubit.take().unwrap()
    }

    fn set_conditional_qubit(&mut self, cq: Qubit) {
        self.conditioned_qubit = Some(cq);
    }
}

impl<'a> UnitaryBuilder for ConditionalContextBuilder<'a> {
    fn with_context(&mut self, q: Qubit) -> ConditionalContextBuilder {
        ConditionalContextBuilder {
            parent_builder: self,
            conditioned_qubit: Some(q),
        }
    }

    fn mat(&mut self, q: Qubit, mat: &[Complex<f64>]) -> Result<Qubit, &'static str> {
        // Special case for applying mat to each qubit in collection.
        if q.indices.len() > 1 && mat.len() == (2 * 2) {
            let qs = self.split_all(q);
            let qs = qs.into_iter().map(|q| self.mat(q, mat).unwrap()).collect();
            Ok(self.merge_with_op(qs, None))
        } else {
            let expected_mat_size = 1 << (2*q.indices.len());
            if expected_mat_size != mat.len() {
                Err("Matrix not of expected size")
            } else {
                let op = self.make_mat_op(&q, mat.to_vec());
                let cq = self.get_conditional_qubit();
                let cq_indices = cq.indices.clone();
                let q = self.merge_with_op(vec![cq, q], Some(op));
                let (cq, q) = self.split_absolute(q, cq_indices).unwrap();

                self.set_conditional_qubit(cq);
                Ok(q)
            }
        }
    }

    fn swap(&mut self, qa: Qubit, qb: Qubit) -> Result<(Qubit, Qubit), &'static str> {
        let op = self.make_swap_op(&qa, &qb)?;
        let cq = self.get_conditional_qubit();
        let cq_indices = cq.indices.clone();
        let qa_indices = qa.indices.clone();
        let q = self.merge_with_op(vec![cq, qa, qb], Some(op));
        let (cq, q) = self.split_absolute(q, cq_indices).unwrap();
        let (qa, qb) = self.split_absolute(q, qa_indices).unwrap();

        self.set_conditional_qubit(cq);
        Ok((qa, qb))
    }

    fn apply_function(&mut self, q_in: Qubit, q_out: Qubit, f: Box<Fn(u64) -> (u64, f64) + Send + Sync>) -> (Qubit, Qubit) {
        let op = self.make_function_op(&q_in, &q_out, f);
        let cq = self.get_conditional_qubit();

        let cq_indices = cq.indices.clone();
        let in_indices = q_in.indices.clone();
        let q = self.merge_with_op(vec![cq, q_in, q_out], Some(op));
        let (cq, q) = self.split_absolute(q, cq_indices).unwrap();
        let (q_in, q_out) = self.split_absolute(q, in_indices).unwrap();

        self.set_conditional_qubit(cq);
        (q_in, q_out)
    }

    fn split_absolute(&mut self, q: Qubit, selected_indices: Vec<u64>) -> Result<(Qubit, Qubit), &'static str> {
        self.parent_builder.split_absolute(q, selected_indices)
    }

    fn make_mat_op(&self, q: &Qubit, data: Vec<Complex<f64>>) -> QubitOp {
        match &self.conditioned_qubit {
            Some(cq) => make_control_op(cq.indices.clone(), self.parent_builder.make_mat_op(q, data)),
            None => panic!("Conditional context builder failed to populate qubit.")
        }
    }

    fn make_swap_op(&self, qa: &Qubit, qb: &Qubit) -> Result<QubitOp, &'static str> {
        match &self.conditioned_qubit {
            Some(cq) => {
                let op = self.parent_builder.make_swap_op(qa, qb)?;
                Ok(make_control_op(cq.indices.clone(), op))
            },
            None => panic!("Conditional context builder failed to populate qubit.")
        }
    }

    fn make_function_op(&self, q_in: &Qubit, q_out: &Qubit, f: Box<Fn(u64) -> (u64, f64) + Send + Sync>) -> QubitOp {
        match &self.conditioned_qubit {
            Some(cq) => {
                let op = self.parent_builder.make_function_op(q_in, q_out, f);
                make_control_op(cq.indices.clone(), op)
            },
            None => panic!("Conditional context builder failed to populate qubit.")
        }
    }

    fn merge_with_op(&mut self, qs: Vec<Qubit>, op: Option<QubitOp>) -> Qubit {
        self.parent_builder.merge_with_op(qs, op)
    }

    fn stochastic_measure(&mut self, q: Qubit) -> (Qubit, u64) {
        self.parent_builder.stochastic_measure(q)
    }
}