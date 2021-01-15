use std::ffi::c_void;

#[derive(Clone)]
pub struct Ptr {
    ptr: *const c_void,
    offset: usize,
}

impl Ptr {
    pub fn new(ptr: *const c_void) -> Self {
        Self { ptr, offset: 0 }
    }

    pub fn advance(&mut self, count: usize) {
        self.offset += count;
    }

    /// # Safety
    /// The safety requirements of [`std::ptr::read`] apply.
    pub unsafe fn read<T>(&mut self) -> T {
        let offset_ptr = self.ptr.add(self.offset);

        self.advance(std::mem::size_of::<T>());
        std::ptr::read(offset_ptr as *const T)
    }

    /// # Safety
    /// The safety requirements of [`read`] apply.
    pub unsafe fn read_i32(&mut self) -> i32 {
        self.read()
    }

    /// # Safety
    /// The safety requirements of [`read`] apply.
    pub unsafe fn read_bool(&mut self) -> bool {
        self.read()
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

pub trait TryFromPtr: Sized {
    type Err;

    /// # Safety
    /// Any implementation can assume the incoming [Ptr] is properly aligned.
    /// This property should also hold when the function returns.
    unsafe fn try_from_ptr(ptr: &mut Ptr) -> Result<Self, Self::Err>;
}
