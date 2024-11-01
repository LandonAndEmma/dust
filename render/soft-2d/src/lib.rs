#![feature(generic_const_exprs, new_zeroed_alloc, portable_simd)]
#![warn(clippy::all)]
#![allow(incomplete_features)]

mod common;
pub mod sync;
#[cfg(feature = "threaded")]
pub mod threaded;
