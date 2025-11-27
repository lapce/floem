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

pub use base::{create_base_signal, BaseSignal, SyncBaseSignal};
pub use context::{provide_context, use_context};
pub use derived::{
    create_derived_rw_signal, create_sync_derived_rw_signal, DerivedRwSignal, SyncDerivedRwSignal,
};
pub use effect::{
    batch, create_effect, create_stateful_updater, create_tracker, create_updater, untrack, Effect,
    SignalTracker, UpdaterEffect,
};
pub use id::Id as ReactiveId;
pub use memo::{create_memo, Memo, SyncMemo};
pub use read::{ReadSignalValue, SignalGet, SignalRead, SignalTrack, SignalWith};
pub use runtime::Runtime;
pub use scope::{as_child_of_current_scope, with_scope, Scope};
pub use signal::{
    create_rw_signal, create_signal, ReadSignal, RwSignal, SyncReadSignal, SyncRwSignal,
    SyncWriteSignal, WriteSignal,
};
pub use trigger::{create_trigger, Trigger};
pub use write::{SignalUpdate, SignalWrite, WriteSignalValue};
