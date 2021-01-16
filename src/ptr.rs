use std::{convert::TryInto, ffi::c_void, fmt::Debug};

use crate::Error;

#[derive(Clone)]
pub struct Ptr {
    ptr: *const c_void,
    offset: usize,
}

impl Ptr {
    pub fn new(ptr: *const c_void) -> Self {
        Self { ptr, offset: 0 }
    }

    pub fn set(&mut self, offset: usize) {
        self.offset = offset;
    }

    pub fn advance(&mut self, count: usize) {
        self.offset += count;
    }

    /// # Safety
    /// The safety requirements of [`std::ptr::read`] apply.
    pub unsafe fn read<T: FromPtr>(&mut self) -> T {
        T::from_ptr(self)
    }

    /// # Safety
    /// The safety requirements of [`std::ptr::read`] apply.
    unsafe fn read_internal<T>(&mut self) -> T {
        let offset_ptr = self.ptr.add(self.offset);

        self.advance(std::mem::size_of::<T>());
        std::ptr::read(offset_ptr as *const T)
    }

    /// # Safety
    /// The safety requirements of [`read`] apply.
    pub unsafe fn try_read<T>(&mut self) -> Result<T, Error>
    where
        T: TryFromPtr<Err = Error>,
    {
        self.scoped(|p| T::try_from_ptr(p))
    }

    pub fn scoped<F, T, E>(&mut self, f: F) -> Result<T, E>
    where
        F: Fn(&mut Ptr) -> Result<T, E>,
    {
        let mut clone = self.clone();

        let res = f(&mut clone);

        match res {
            ok @ Ok(_) => {
                *self = clone;
                ok
            }
            x => x,
        }
    }
}

pub trait FromPtr {
    /// # Safety
    /// Any implementation can assume the incoming [Ptr] is properly aligned.
    /// This property should also hold when the function returns.
    unsafe fn from_ptr(ptr: &mut Ptr) -> Self;
}

impl<T, const N: usize> FromPtr for [T; N]
where
    T: FromPtr + Debug,
{
    unsafe fn from_ptr(ptr: &mut Ptr) -> Self {
        let mut values = Vec::with_capacity(N);
        for _ in 0..N {
            values.push(T::from_ptr(ptr))
        }
        values.try_into().unwrap()
    }
}

macro_rules! impl_from_ptr {
    ($ty:ty) => {
        impl $crate::ptr::FromPtr for $ty {
            unsafe fn from_ptr(ptr: &mut $crate::ptr::Ptr) -> Self {
                ptr.read_internal()
            }
        }
    };
}

impl_from_ptr!(u8);
impl_from_ptr!(i32);
impl_from_ptr!(bool);

pub trait TryFromPtr: Sized {
    type Err;

    /// # Safety
    /// Any implementation can assume the incoming [Ptr] is properly aligned.
    /// This property should also hold when the function returns.
    unsafe fn try_from_ptr(ptr: &mut Ptr) -> Result<Self, Self::Err>;
}
