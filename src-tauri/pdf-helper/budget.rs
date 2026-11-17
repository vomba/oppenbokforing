//! Counts requested Rust payload, NOT RSS, C malloc, stacks or allocator overhead.
//! Fixed from process startup, including before main. Realloc reserves both blocks
//! until success, covering a possible allocate/copy/free transient conservatively.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct Budget {
    live: AtomicUsize,
    limit: usize,
}
impl Budget {
    pub const fn new(limit: usize) -> Self {
        Self {
            live: AtomicUsize::new(0),
            limit,
        }
    }
    fn reserve(&self, size: usize) -> bool {
        let mut live = self.live.load(Ordering::Relaxed);
        loop {
            let Some(next) = live.checked_add(size).filter(|n| *n <= self.limit) else {
                return false;
            };
            match self
                .live
                .compare_exchange_weak(live, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return true,
                Err(actual) => live = actual,
            }
        }
    }
    fn release(&self, size: usize) {
        self.live.fetch_sub(size, Ordering::Relaxed);
    }
}
// SAFETY: all layouts/pointers are forwarded unchanged to System. Reservations
// are atomic, checked before allocation, and rolled back on failure. Dealloc and
// successful realloc release exactly the matching old layout, never reset live.
unsafe impl GlobalAlloc for Budget {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.reserve(layout.size()) {
            return ptr::null_mut();
        }
        let p = System.alloc(layout);
        if p.is_null() {
            self.release(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if !self.reserve(layout.size()) {
            return ptr::null_mut();
        }
        let p = System.alloc_zeroed(layout);
        if p.is_null() {
            self.release(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        self.release(layout.size());
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if !self.reserve(new_size) {
            return ptr::null_mut();
        }
        let next = System.realloc(p, layout, new_size);
        self.release(if next.is_null() {
            new_size
        } else {
            layout.size()
        });
        next
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alloc_zeroed_realloc_denial_and_reuse() {
        let budget = Budget::new(100);
        let layout = Layout::from_size_align(40, 8).unwrap();
        unsafe {
            let p = budget.alloc_zeroed(layout);
            assert!(!p.is_null());
            assert_eq!(*p, 0);
            assert!(budget.realloc(p, layout, 61).is_null());
            assert_eq!(budget.live.load(Ordering::Relaxed), 40);
            let q = budget.realloc(p, layout, 60);
            assert!(!q.is_null());
            assert_eq!(budget.live.load(Ordering::Relaxed), 60);
            let r = budget.alloc_zeroed(layout);
            assert!(!r.is_null());
            assert!(budget.alloc(layout).is_null());
            budget.dealloc(r, layout);
            budget.dealloc(q, Layout::from_size_align(60, 8).unwrap());
            assert_eq!(budget.live.load(Ordering::Relaxed), 0);
        }
    }
    #[test]
    fn checked_reservations_never_oversubscribe_or_overflow() {
        let budget = Budget::new(64);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let b = &budget;
                scope.spawn(move || {
                    for _ in 0..10_000 {
                        if b.reserve(17) {
                            assert!(b.live.load(Ordering::Relaxed) <= 64);
                            b.release(17);
                        }
                    }
                });
            }
        });
        assert_eq!(budget.live.load(Ordering::Relaxed), 0);
        assert!(!budget.reserve(usize::MAX));
        let overflow = Budget::new(usize::MAX);
        assert!(overflow.reserve(usize::MAX));
        assert!(!overflow.reserve(1));
        overflow.release(usize::MAX);
        assert_eq!(overflow.live.load(Ordering::Relaxed), 0);
        unsafe {
            let l = Layout::from_size_align(64, 8).unwrap();
            let p = budget.alloc(l);
            assert!(!p.is_null());
            assert!(budget.alloc_zeroed(l).is_null());
            budget.dealloc(p, l);
            let p = budget.alloc_zeroed(l);
            assert!(!p.is_null());
            budget.dealloc(p, l);
        }
    }
}
