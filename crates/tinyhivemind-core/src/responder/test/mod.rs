//! Unit tests for the deterministic responder ladder, split by behavior:
//! ladder routing, selector-output acceptance, wire-form pinning, and the
//! ADR 0009 disclosure classification of each outcome. Shared fixtures and
//! wire-assertion helpers live in [`support`].

mod disclosure;
mod ladder;
mod selection;
mod support;
mod wire;
