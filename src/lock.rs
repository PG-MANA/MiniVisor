//!
//! Spin Lock による Mutex<T> の実装
//!

use crate::asm::{get_daif_and_disable_irq_fiq, set_daif};

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct Mutex<T: ?Sized> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

pub struct MutexGuard<'a, T: ?Sized + 'a> {
    lock: &'a AtomicBool,
    daif: u64,
    data: &'a mut T,
    _forbid_send: PhantomData<*const ()>,
}

impl<T> Mutex<T> {
    pub const fn new(d: T) -> Mutex<T> {
        Mutex {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(d),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            while self.lock.load(Ordering::Relaxed) {
                spin_loop();
            }
            let daif = unsafe { get_daif_and_disable_irq_fiq() };
            if self
                .lock
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return MutexGuard {
                    lock: &self.lock,
                    daif,
                    data: unsafe { &mut *self.data.get() },
                    _forbid_send: PhantomData,
                };
            }
            unsafe { set_daif(daif) };
        }
    }
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

//impl<T: ?Sized> !Send for MutexGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.data
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.data
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
        unsafe { set_daif(self.daif) };
    }
}
