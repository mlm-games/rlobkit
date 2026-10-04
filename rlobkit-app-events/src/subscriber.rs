use std::sync::{Arc, Mutex, MutexGuard};

type Callback<T> = Arc<dyn Fn(T) + Send + Sync>;

/// The change listener a state module publishes to.
///
/// A single replaceable slot rather than a `OnceLock`, so re-registering after a
/// UI reload or a second framework layer attaches works instead of silently
/// dropping the subscriber. The previous callback is dropped here, which is what
/// makes replacement possible.
pub(crate) struct Subscriber<T: Copy + Send + 'static> {
    slot: Mutex<Option<Callback<T>>>,
}

impl<T: Copy + Send + 'static> Subscriber<T> {
    pub(crate) const fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    pub(crate) fn set(&self, cb: impl Fn(T) + Send + Sync + 'static) {
        lock(&self.slot).replace(Arc::new(cb));
    }

    /// Clone the callback out before invoking it: the callback runs on the JNI
    /// thread and may re-enter this module (reading the cached value, or
    /// replacing itself), which would deadlock on the lock.
    pub(crate) fn notify(&self, value: T) {
        let cb = { lock(&self.slot).clone() };
        if let Some(cb) = cb {
            cb(value);
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
