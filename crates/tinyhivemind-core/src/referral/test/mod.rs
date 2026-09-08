//! Unit tests for bounded cross-desk referral, split by behavior: forward
//! routing and its refusals, carrying an answer back, wire-form pinning, and
//! the ADR 0009 disclosure classification of each refusal reason. Shared
//! fixtures live in [`support`].

mod disclosure;
mod returns;
mod routing;
mod support;
mod wire;
