//! Local evaluation of a problem's solution against `testcases.txt`.
//!
//! Everything here is language-agnostic: a language runtime builds and runs
//! the solution and hands back an [`verdict::Execution`]; parsing the answer off
//! stdout, comparing it and assigning a [`verdict::Verdict`] are shared.

pub mod compare;
pub mod protocol;
pub mod verdict;
