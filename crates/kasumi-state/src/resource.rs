//! Retained asynchronous resources.
//!
//! A [`Resource<T>`] stores the last successful value, the current loading
//! state and the last error behind an `Arc`. Loaders run on a shared background
//! executor; the UI thread only reads snapshots. A [`ResourceStore`] caches
//! handles by `Id`, starts a load at most once per slot, and exposes a wake
//! signal the runtime can use to schedule a repaint when a load completes.
//!
//! Loaders are `Fn`, not `FnOnce`, so a resource can be reloaded or retried.
//! They must be `Send + Sync + 'static`; values crossing back to the UI must be
//! `Send + 'static`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use kasumi_core::Id;

use crate::executor;

/// How a resource currently stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceStatus {
    /// No load has been requested, or a running load was cancelled.
    Idle,
    /// A load is running. A previously loaded value may still be available.
    Loading,
    /// The most recent load succeeded.
    Ready,
    /// The most recent load failed. A previous value may still be available.
    Failed,
}

impl ResourceStatus {
    pub fn is_loading(self) -> bool {
        matches!(self, Self::Loading)
    }

    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn is_failed(self) -> bool {
        matches!(self, Self::Failed)
    }
}

/// A loader failure, rendered lazily to a message.
#[derive(Clone, PartialEq, Eq)]
pub struct ResourceError {
    message: Arc<str>,
}

impl ResourceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into().into(),
        }
    }

    fn from_display(error: impl fmt::Display) -> Self {
        Self::new(error.to_string())
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ResourceError").field(&self.message).finish()
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ResourceError {}

/// Wakes the presentation backend when a resource finishes loading.
///
/// Backends install one of these on the runtime; it is usually an
/// `EventLoopProxy::send_event` wrapper.
pub type ResourceWaker = Arc<dyn Fn() + Send + Sync>;

type Loader<T> = Arc<dyn Fn() -> Result<T, ResourceError> + Send + Sync>;

struct ResourceData<T> {
    status: ResourceStatus,
    value: Option<T>,
    error: Option<ResourceError>,
    /// Incremented whenever a new load starts or the value is replaced. A
    /// worker only publishes a result when its generation still matches.
    generation: u64,
}

struct ResourceInner<T> {
    data: Mutex<ResourceData<T>>,
    loader: Loader<T>,
    wake: Option<ResourceWaker>,
}

/// A cloneable handle to one asynchronously loaded value.
///
/// `get`/`with` expose the last successfully loaded value even while a reload
/// is in flight, which lets the UI show stale data while refreshing. Use
/// [`Resource::status`] to distinguish fresh from stale data.
pub struct Resource<T> {
    inner: Arc<ResourceInner<T>>,
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Send + 'static> fmt::Debug for Resource<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resource")
            .field("status", &self.status())
            .field("has_value", &self.has_value())
            .finish()
    }
}

impl<T: Send + 'static> Resource<T> {
    /// Starts a load immediately.
    ///
    /// The closure runs on a background worker. It should return `Ok(value)`
    /// or an error that implements `Display`; errors are flattened into a
    /// [`ResourceError`] because they must cross thread and frame boundaries.
    pub fn load<F, E>(loader: F) -> Self
    where
        F: Fn() -> Result<T, E> + Send + Sync + 'static,
        E: fmt::Display + 'static,
    {
        Self::load_with_wake(loader, None)
    }

    /// Starts a future on the background executor.
    ///
    /// The future is polled to completion on a worker thread. Futures that
    /// depend on an external reactor (for example a tokio `TcpStream`) must
    /// bring that runtime with them; this executor only polls and parks.
    pub fn load_future<F, Fut, E>(factory: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        E: fmt::Display + 'static,
    {
        Self::load_future_with_wake(factory, None)
    }

    #[doc(hidden)]
    pub fn load_with_wake<F, E>(loader: F, wake: Option<ResourceWaker>) -> Self
    where
        F: Fn() -> Result<T, E> + Send + Sync + 'static,
        E: fmt::Display + 'static,
    {
        let loader: Loader<T> = Arc::new(move || loader().map_err(ResourceError::from_display));
        Self::from_loader(loader, wake)
    }

    #[doc(hidden)]
    pub fn load_future_with_wake<F, Fut, E>(factory: F, wake: Option<ResourceWaker>) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        E: fmt::Display + 'static,
    {
        let loader: Loader<T> =
            Arc::new(move || executor::block_on(factory()).map_err(ResourceError::from_display));
        Self::from_loader(loader, wake)
    }

    fn from_loader(loader: Loader<T>, wake: Option<ResourceWaker>) -> Self {
        let inner = Arc::new(ResourceInner {
            data: Mutex::new(ResourceData {
                status: ResourceStatus::Loading,
                value: None,
                error: None,
                generation: 1,
            }),
            loader,
            wake,
        });
        Self::launch(&inner, 1);
        Self { inner }
    }

    fn launch(inner: &Arc<ResourceInner<T>>, generation: u64) {
        let weak = Arc::downgrade(inner);
        let loader = Arc::clone(&inner.loader);
        executor::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader()))
                .unwrap_or_else(|_| Err(ResourceError::new("resource loader panicked")));
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let published = {
                let mut data = lock(&inner.data);
                if data.generation != generation {
                    false
                } else {
                    match result {
                        Ok(value) => {
                            data.value = Some(value);
                            data.error = None;
                            data.status = ResourceStatus::Ready;
                        }
                        Err(error) => {
                            data.error = Some(error);
                            data.status = ResourceStatus::Failed;
                        }
                    }
                    true
                }
            };
            if published && let Some(wake) = &inner.wake {
                wake();
            }
        });
    }

    /// Re-runs the loader, keeping the last successful value available.
    pub fn reload(&self) {
        let generation = {
            let mut data = lock(&self.inner.data);
            data.generation = data.generation.wrapping_add(1);
            data.status = ResourceStatus::Loading;
            data.error = None;
            data.generation
        };
        Self::launch(&self.inner, generation);
        self.notify();
    }

    /// Discards the current value and invalidates any in-flight load.
    pub fn cancel(&self) {
        {
            let mut data = lock(&self.inner.data);
            data.generation = data.generation.wrapping_add(1);
            data.value = None;
            data.error = None;
            data.status = ResourceStatus::Idle;
        }
        self.notify();
    }

    /// Replaces the value immediately and invalidates any in-flight load.
    ///
    /// Useful for optimistic updates; a later [`Resource::reload`] still works.
    pub fn set(&self, value: T) {
        {
            let mut data = lock(&self.inner.data);
            data.generation = data.generation.wrapping_add(1);
            data.value = Some(value);
            data.error = None;
            data.status = ResourceStatus::Ready;
        }
        self.notify();
    }

    fn notify(&self) {
        if let Some(wake) = &self.inner.wake {
            wake();
        }
    }

    pub fn status(&self) -> ResourceStatus {
        lock(&self.inner.data).status
    }

    pub fn is_idle(&self) -> bool {
        self.status() == ResourceStatus::Idle
    }

    pub fn is_loading(&self) -> bool {
        self.status() == ResourceStatus::Loading
    }

    pub fn is_ready(&self) -> bool {
        self.status() == ResourceStatus::Ready
    }

    pub fn is_failed(&self) -> bool {
        self.status() == ResourceStatus::Failed
    }

    /// The last error, if the most recent load failed.
    pub fn error(&self) -> Option<ResourceError> {
        lock(&self.inner.data).error.clone()
    }

    /// True when a value is available, even if a reload is in flight.
    pub fn has_value(&self) -> bool {
        lock(&self.inner.data).value.is_some()
    }

    /// Borrows the last successful value.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let data = lock(&self.inner.data);
        data.value.as_ref().map(f)
    }

    /// Clones the last successful value.
    pub fn get(&self) -> Option<T>
    where
        T: Clone,
    {
        self.with(T::clone)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

struct Entry {
    handle: Box<dyn Any>,
    type_name: &'static str,
    path: String,
    last_seen: u64,
}

struct WakeState {
    notified: AtomicBool,
    callback: Mutex<Option<ResourceWaker>>,
}

impl WakeState {
    fn new() -> Self {
        Self {
            notified: AtomicBool::new(false),
            callback: Mutex::new(None),
        }
    }

    fn notify(&self) {
        self.notified.store(true, Ordering::Release);
        let callback = lock(&self.callback).clone();
        if let Some(callback) = callback {
            callback();
        }
    }

    fn take(&self) -> bool {
        self.notified.swap(false, Ordering::AcqRel)
    }

    fn set_callback(&self, callback: Option<ResourceWaker>) {
        *lock(&self.callback) = callback;
    }
}

/// Cache of resource handles addressed by element-scoped ids.
///
/// The store is owned by a runtime and used from the UI thread. Workers only
/// touch the handles' internal mutexes and the shared wake flag.
pub struct ResourceStore {
    entries: RefCell<HashMap<Id, Entry>>,
    frame: Cell<u64>,
    ttl_frames: Cell<u64>,
    wake: Arc<WakeState>,
    wake_fn: ResourceWaker,
}

impl Default for ResourceStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceStore {
    pub fn new() -> Self {
        let wake = Arc::new(WakeState::new());
        let wake_fn = {
            let wake = Arc::clone(&wake);
            Arc::new(move || wake.notify()) as ResourceWaker
        };
        Self {
            entries: RefCell::new(HashMap::new()),
            frame: Cell::new(0),
            ttl_frames: Cell::new(crate::DEFAULT_TTL_FRAMES),
            wake,
            wake_fn,
        }
    }

    /// Installs the backend wake callback used when a load finishes.
    pub fn set_wake_callback(&self, callback: Option<ResourceWaker>) {
        self.wake.set_callback(callback);
    }

    pub fn begin_frame(&self, frame: u64) {
        self.frame.set(frame);
    }

    /// Drops resource slots that have not been declared for the TTL window.
    ///
    /// External handles stay valid; if a slot is requested again after
    /// eviction, its loader runs from scratch.
    pub fn end_frame(&self) {
        let frame = self.frame.get();
        let ttl = self.ttl_frames.get();
        let mut entries = match self.entries.try_borrow_mut() {
            Ok(entries) => entries,
            Err(_) => return,
        };
        entries.retain(|id, entry| {
            let keep = frame.saturating_sub(entry.last_seen) <= ttl;
            if !keep && std::env::var_os("KASUMI_RESOURCE_TRACE").is_some() {
                eprintln!(
                    "kasumi: evicted resource {id} ({}), last used at frame {}",
                    entry.path, entry.last_seen
                );
            }
            keep
        });
    }

    pub fn set_ttl_frames(&self, frames: u64) {
        self.ttl_frames.set(frames);
    }

    pub fn entry_count(&self) -> usize {
        self.entries.borrow().len()
    }

    /// True when at least one load completed since the last call.
    pub fn take_notified(&self) -> bool {
        self.wake.take()
    }

    /// Returns the cached handle for `id`, creating and starting it when the
    /// slot is empty. A type mismatch panics with both paths.
    pub fn get_or_insert<T, F, E>(&self, id: Id, path: impl Into<String>, loader: F) -> Resource<T>
    where
        T: Send + 'static,
        F: Fn() -> Result<T, E> + Send + Sync + 'static,
        E: fmt::Display + 'static,
    {
        let wake = Arc::clone(&self.wake_fn);
        self.get_or_insert_with(id, path, || Resource::load_with_wake(loader, Some(wake)))
    }

    /// Future-backed variant of [`ResourceStore::get_or_insert`].
    pub fn get_or_insert_future<T, F, Fut, E>(
        &self,
        id: Id,
        path: impl Into<String>,
        factory: F,
    ) -> Resource<T>
    where
        T: Send + 'static,
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + 'static,
        E: fmt::Display + 'static,
    {
        let wake = Arc::clone(&self.wake_fn);
        self.get_or_insert_with(id, path, || {
            Resource::load_future_with_wake(factory, Some(wake))
        })
    }

    fn get_or_insert_with<T: Send + 'static>(
        &self,
        id: Id,
        path: impl Into<String>,
        create: impl FnOnce() -> Resource<T>,
    ) -> Resource<T> {
        let frame = self.frame.get();
        if let Some(entry) = self.entries.borrow_mut().get_mut(&id) {
            entry.last_seen = frame;
            let Some(handle) = entry.handle.downcast_ref::<Resource<T>>() else {
                panic!(
                    "kasumi: resource type mismatch at {id} ({}). Expected `{}`, found `{}`.",
                    entry.path,
                    std::any::type_name::<T>(),
                    entry.type_name
                );
            };
            return handle.clone();
        }
        let path = path.into();
        let resource = create();
        self.entries.borrow_mut().insert(
            id,
            Entry {
                handle: Box::new(resource.clone()),
                type_name: std::any::type_name::<T>(),
                path,
                last_seen: frame,
            },
        );
        resource
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    fn wait_for<T: Send + 'static>(
        resource: &Resource<T>,
        mut predicate: impl FnMut(ResourceStatus) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if predicate(resource.status()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!(
            "resource did not reach the expected state: {:?}",
            resource.status()
        );
    }

    #[test]
    fn resource_loads_reports_errors_and_retries() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let resource = {
            let attempts = Arc::clone(&attempts);
            Resource::load(move || {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    Err("network down".to_owned())
                } else {
                    Ok(41 + attempt as i32)
                }
            })
        };
        wait_for(&resource, |status| status == ResourceStatus::Failed);
        assert_eq!(resource.error().unwrap().message(), "network down");
        assert!(!resource.has_value());

        resource.reload();
        wait_for(&resource, |status| status == ResourceStatus::Ready);
        assert_eq!(resource.get(), Some(42));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);

        // Reload keeps the old value visible while the next generation runs.
        resource.reload();
        assert_eq!(resource.get(), Some(42));
        wait_for(&resource, |status| status == ResourceStatus::Ready);
    }

    #[test]
    fn cancel_discards_an_in_flight_result() {
        let resource = Resource::load(|| {
            std::thread::sleep(Duration::from_millis(40));
            Ok::<_, String>(9u32)
        });
        assert!(resource.is_loading());
        resource.cancel();
        assert!(resource.is_idle());
        std::thread::sleep(Duration::from_millis(80));
        assert!(resource.is_idle(), "cancelled worker must not publish");
        assert!(!resource.has_value());
    }

    #[test]
    fn future_resources_complete_on_the_executor() {
        let resource = Resource::load_future(|| async {
            std::thread::sleep(Duration::from_millis(10));
            Ok::<_, String>("future value")
        });
        wait_for(&resource, |status| status == ResourceStatus::Ready);
        assert_eq!(resource.get().as_deref(), Some("future value"));
    }

    #[test]
    fn store_caches_by_id_and_evicts_unused_entries() {
        let store = ResourceStore::new();
        store.begin_frame(1);
        let loads = Arc::new(AtomicUsize::new(0));
        let id = Id::from_raw(77);
        let resource = {
            let loads = Arc::clone(&loads);
            store.get_or_insert(id, "test-resource", move || {
                loads.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>(5u32)
            })
        };
        let same = store.get_or_insert(id, "test-resource", || Ok::<_, String>(99u32));
        wait_for(&resource, |status| status == ResourceStatus::Ready);
        assert_eq!(same.get(), Some(5));
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_eq!(store.entry_count(), 1);

        store.set_ttl_frames(0);
        store.begin_frame(2);
        store.end_frame();
        assert_eq!(store.entry_count(), 0);
        let reload_loads = Arc::clone(&loads);
        let reloaded = store.get_or_insert(id, "test-resource", move || {
            reload_loads.fetch_add(1, Ordering::SeqCst);
            Ok::<_, String>(6u32)
        });
        wait_for(&reloaded, |status| status == ResourceStatus::Ready);
        assert_eq!(reloaded.get(), Some(6));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn store_forwards_completion_wake() {
        let store = ResourceStore::new();
        let woken = Arc::new(AtomicBool::new(false));
        {
            let woken = Arc::clone(&woken);
            store.set_wake_callback(Some(Arc::new(move || {
                woken.store(true, Ordering::SeqCst);
            })));
        }
        store.begin_frame(1);
        let resource = store.get_or_insert(Id::from_raw(88), "wake", || Ok::<_, String>(1u8));
        wait_for(&resource, |status| status == ResourceStatus::Ready);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !woken.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(woken.load(Ordering::SeqCst));
        assert!(store.take_notified());
        assert!(!store.take_notified());
    }
}
