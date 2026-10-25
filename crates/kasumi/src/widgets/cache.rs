use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use kasumi_core::ImageData;

const CACHE_CAPACITY: usize = 16;

thread_local! {
    static IMAGES: RefCell<VecDeque<(u64, Arc<ImageData>)>> = const {
        RefCell::new(VecDeque::new())
    };
}

/// Reuses CPU-side rasters created from declarative builders such as SVG and
/// QR. The key includes every input that affects the pixels.
pub(crate) fn image(key: u64, create: impl FnOnce() -> ImageData) -> Arc<ImageData> {
    IMAGES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache.iter().position(|(cached, _)| *cached == key) {
            let entry = cache.remove(index).expect("index came from this cache");
            let image = Arc::clone(&entry.1);
            cache.push_front(entry);
            return image;
        }
        let image = Arc::new(create());
        cache.push_front((key, Arc::clone(&image)));
        if cache.len() > CACHE_CAPACITY {
            cache.pop_back();
        }
        image
    })
}
