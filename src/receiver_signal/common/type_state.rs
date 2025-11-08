// ============================================================================
// Type-state markers (shared across all async signal types)
// ============================================================================

/// Type-state marker for event loop executor.
pub struct EventLoopExecutor;

/// Type-state marker for tokio executor.
#[cfg(feature = "tokio")]
pub struct TokioExecutor;

/// Type-state marker for tokio executor.
#[cfg(feature = "tokio")]
pub struct TokioBlockingExecutor;

/// Type-state marker for custom executor.
pub struct CustomExecutor<F>(pub F);

/// Type-state marker for no initial value.
pub struct NoInitial;

/// Type-state marker for having an initial value.
pub struct WithInitialValue<T>(pub T);

/// Type-state marker for std::thread executor (used by ChannelSignal).
pub struct StdThreadExecutor;
