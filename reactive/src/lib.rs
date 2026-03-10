//! # Floem Reactive
//!
//! [`RwSignal::new_split`](RwSignal::new_split) returns a separated [`ReadSignal`] and [`WriteSignal`] for a variable.
//! An existing `RwSignal` may be converted using [`RwSignal::read_only`](RwSignal::read_only)
//! and [`RwSignal::write_only`](RwSignal::write_only) where necessary, but the reverse is not possible.

#![allow(deprecated)]

mod base;
mod context;
mod derived;
mod effect;
mod id;
mod impls;
mod memo;
mod read;
mod runtime;
mod scope;
mod signal;
mod storage;
mod sync_runtime;
mod trigger;
mod write;

pub use base::{BaseSignal, SyncBaseSignal, create_base_signal};
pub use context::{Context, provide_context, use_context};
pub use derived::{
    DerivedRwSignal, SyncDerivedRwSignal, create_derived_rw_signal, create_sync_derived_rw_signal,
};
pub use effect::{
    Effect, EffectTrait, SignalTracker, UpdaterEffect, batch, create_effect,
    create_stateful_updater, create_tracker, create_updater, untrack,
};
pub use id::Id as ReactiveId;
pub use memo::{Memo, create_memo};
pub use read::{ReadRef, SignalGet, SignalRead, SignalTrack, SignalWith};
pub use runtime::Runtime;
pub use scope::{Scope, as_child_of_current_scope, with_scope};
pub use signal::{
    ReadSignal, RwSignal, SyncReadSignal, SyncRwSignal, SyncWriteSignal, WriteSignal,
    create_rw_signal, create_signal,
};
pub use trigger::{Trigger, create_trigger};
pub use write::{SignalUpdate, SignalWrite, WriteRef};
