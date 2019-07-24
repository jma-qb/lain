use crate::NewFuzzed;
use crate::mutator::{Mutator, MutatorMode};
use crate::rand::seq::index;
use crate::rand::Rng;
use crate::traits::*;
use crate::types::*;

use num_traits::{Bounded, NumCast};
use num_traits::{WrappingAdd, WrappingSub};
use std::ops::BitXor;
use std::cmp::min;

// we'll shrink by a factor of 1/4, 1/2, 3/4, or down to [0, 8] bytes
#[derive(Copy, Clone,NewFuzzed, PartialEq)]
enum VecResizeCount {
    Quarter,
    Half,
    ThreeQuarters,
    FixedBytes,
    AllBytes,
}

#[derive(Copy, Clone, NewFuzzed)]
enum VecResizeDirection {
    FromBeginning,
    FromEnd,
}

#[derive(Copy, Clone, PartialEq, NewFuzzed)]
enum VecResizeType {
    Grow,
    Shrink,
}

/// Grows a `Vec`.
/// This will randomly select to grow by a factor of 1/4, 1/2, 3/4, or a fixed number of bytes
/// in the range of [1, 8]. Elements may be added randomly to the beginning or end of the the vec
fn grow_vec<T: NewFuzzed + SerializedSize, R: Rng>(vec: &mut Vec<T>, mutator: &mut Mutator<R>, mut max_size: Option<usize>) {
    let resize_count = VecResizeCount::new_fuzzed(mutator, None);
    let mut num_elements = if vec.len() == 0 {
        mutator.gen_range(1, 9)
    } else {
        match resize_count {
            VecResizeCount::Quarter => {
                vec.len() / 4
            }
            VecResizeCount::Half => {
                vec.len() / 2
            }
            VecResizeCount::ThreeQuarters => {
                vec.len() - (vec.len() / 4)
            }
            VecResizeCount::FixedBytes => {
                mutator.gen_range(1, 9)
            }
            VecResizeCount::AllBytes => {
                vec.len()
            }
        }
    };

    // If we were given a size constraint, we need to respect it
    if let Some(max_size) = max_size {
        num_elements = min(num_elements, max_size / T::min_nonzero_elements_size());
    }

    if num_elements == 0 {
        return;
    }

    match VecResizeDirection::new_fuzzed(mutator, None) {
        VecResizeDirection::FromBeginning => {
            // to avoid shifting the the entire vec on every iteration, we will
            // instead allocate a new vec, then extend it with the previous one
            let mut new_vec = Vec::with_capacity(num_elements);
            for _i in 0..num_elements {
                let constraints = max_size.map_or(None, |max_size| {
                    let mut c = Constraints::new();
                    c.max_size(max_size);

                    Some(c)
                });

                let element = T::new_fuzzed(mutator, constraints.as_ref());
                if let Some(inner_max_size) = max_size {
                    // if this element is larger than the size we're allotted,
                    // then let's just exit
                    let element_size = element.serialized_size();
                    if element_size > inner_max_size {
                        break;
                    }

                    max_size = Some(inner_max_size - element_size)
                }

                new_vec.push(element);
            }

            new_vec.append(vec);
            *vec = new_vec
        }
        VecResizeDirection::FromEnd => {
            for _i in 0..num_elements {
                let constraints = max_size.map_or(None, |max_size| {
                    let mut c = Constraints::new();
                    c.max_size(max_size);

                    Some(c)
                });

                let element = T::new_fuzzed(mutator, constraints.as_ref());
                if let Some(inner_max_size) = max_size {
                    // if this element is larger than the size we're allotted,
                    // then let's just exit
                    let element_size = element.serialized_size();
                    if element_size > inner_max_size {
                        break;
                    }

                    max_size = Some(inner_max_size - element_size)
                }

                vec.push(element);
            }
        }
    }
}

/// Shrinks a `Vec`.
/// This will randomly select to resize by a factor of 1/4, 1/2, 3/4, or a fixed number of bytes
/// in the range of [1, 8]. Elements may be removed randomly from the beginning or end of the the vec
fn shrink_vec<T, R: Rng>(vec: &mut Vec<T>, mutator: &mut Mutator<R>) {
    if vec.len() == 0 {
        return;
    }

    let resize_count = VecResizeCount::new_fuzzed(mutator, None);
    let mut num_elements = match resize_count {
        VecResizeCount::Quarter => {
            vec.len() / 4
        }
        VecResizeCount::Half => {
            vec.len() / 2
        }
        VecResizeCount::ThreeQuarters => {
            vec.len() - (vec.len() / 4)
        }
        VecResizeCount::FixedBytes => {
            mutator.gen_range(1, 9)
        }
        VecResizeCount::AllBytes => {
            vec.len()
        }
    };

    if num_elements == 0 {
        num_elements = mutator.gen_range(0, vec.len() + 1);
    }

    // Special case probably isn't required here, but better to be explicit
    if num_elements == vec.len() {
        vec.drain(..);
        return;
    }

    match VecResizeDirection::new_fuzzed(mutator, None) {
        VecResizeDirection::FromBeginning => {
            vec.drain(0..num_elements);
        }
        VecResizeDirection::FromEnd => {
            vec.drain(vec.len()-num_elements..);
        }
    }
}

impl<T> Mutatable for Vec<T>
where
    T: Mutatable, 
{
    default fn mutate<R: rand::Rng>(
        &mut self,
        mutator: &mut Mutator<R>,
        constraints: Option<&Constraints<u8>>,
    ) {
        // 1% chance to resize this vec
        if mutator.mode() == MutatorMode::Havoc && mutator.gen_chance(1.0) {
            shrink_vec(self, mutator);
        } else {
            self.as_mut_slice().mutate(mutator, constraints);
        }
    }
}

impl<T> Mutatable for Vec<T>
where
    T: Mutatable + NewFuzzed + SerializedSize, 
{
    fn mutate<R: rand::Rng>(
        &mut self,
        mutator: &mut Mutator<R>,
        constraints: Option<&Constraints<u8>>,
    ) {
        // 1% chance to resize this vec
        if mutator.mode() == MutatorMode::Havoc && mutator.gen_chance(1.0) {
            let resize_type = VecResizeType::new_fuzzed(mutator, None);
            if resize_type == VecResizeType::Grow {
                grow_vec(self, mutator, constraints.map_or(None, |c| c.max_size));
            } else {
                shrink_vec(self, mutator);
            }
        } else {
            self.as_mut_slice().mutate(mutator, constraints);
        }
    }
}

impl<T> Mutatable for [T]
where
    T: Mutatable,
{
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, constraints: Option<&Constraints<u8>>) {
        for item in self.iter_mut() {
            T::mutate(item, mutator, constraints);
        }
    }
}

impl Mutatable for bool {
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        *self = mutator.gen_range(0u8, 2u8) != 0;
    }
}

impl<T, I> Mutatable for UnsafeEnum<T, I>
where
    T: ToPrimitive<I>,
    I: BitXor<Output = I>
        + NumCast
        + Bounded
        + Copy
        + DangerousNumber<I>
        + std::fmt::Display
        + WrappingAdd
        + WrappingSub,
{
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        if let UnsafeEnum::Valid(ref value) = *self {
            *self = UnsafeEnum::Invalid(value.to_primitive());
        }

        match *self {
            UnsafeEnum::Invalid(ref mut value) => {
                mutator.mutate_from_mutation_mode(value);
            }
            _ => unreachable!(),
        }
    }
}

impl Mutatable for AsciiString {
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        trace!("performing mutation on an AsciiString");

        // TODO: Implement logic for resizing?
        let num_mutations = mutator.gen_range(1, self.inner.len());
        for idx in index::sample(&mut mutator.rng, self.inner.len(), num_mutations).iter() {
            self.inner[idx] = AsciiChar::new_fuzzed(mutator, None);
        }
    }
}

impl Mutatable for Utf8String {
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        trace!("performing mutation on a Utf8String");

        // TODO: Implement logic for resizing?
        let num_mutations = mutator.gen_range(1, self.inner.len());
        for idx in index::sample(&mut mutator.rng, self.inner.len(), num_mutations).iter() {
            self.inner[idx] = Utf8Char::new_fuzzed(mutator, None);
        }
    }
}

macro_rules! impl_mutatable {
    ( $($name:ident),* ) => {
        $(
            impl Mutatable for $name {
                #[inline(always)]
                fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
                    mutator.mutate_from_mutation_mode(self);
                }
            }
        )*
    }
}

impl_mutatable!(u64, u32, u16, u8);

impl Mutatable for i8 {
    #[inline(always)]
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        let mut val = *self as u8;
        mutator.mutate_from_mutation_mode(&mut val);
        *self = val as i8;
    }
}

impl Mutatable for i16 {
    #[inline(always)]
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        let mut val = *self as u16;
        mutator.mutate_from_mutation_mode(&mut val);
        *self = val as i16;
    }
}

impl Mutatable for i32 {
    #[inline(always)]
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        let mut val = *self as u32;
        mutator.mutate_from_mutation_mode(&mut val);
        *self = val as i32;
    }
}

impl Mutatable for i64 {
    #[inline(always)]
    fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, _constraints: Option<&Constraints<u8>>) {
        let mut val = *self as u64;
        mutator.mutate_from_mutation_mode(&mut val);
        *self = val as i64;
    }
}


impl<T> Mutatable for [T; 0]
where
    T: Mutatable,
{
    fn mutate<R: Rng>(
        &mut self,
        _mutator: &mut Mutator<R>,
        _constraints: Option<&Constraints<u8>>,
    ) {
        // nop
    }
}

macro_rules! impl_mutatable_array {
    ( $($size:expr),* ) => {
        $(
            impl<T> Mutatable for [T; $size]
            where T: Mutatable {
                #[inline(always)]
                fn mutate<R: Rng>(&mut self, mutator: &mut Mutator<R>, constraints: Option<&Constraints<u8>>) {
                    // Treat this as a slice
                    self[0..].mutate(mutator, constraints);
                }
            }
        )*
    }
}

impl_mutatable_array!(
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50,
    51, 52, 53, 54, 55, 56, 57, 58, 59, 60
);
