use std::ptr::NonNull;

use crate::SimdLane;

const fn compute_align<S>() -> usize {
    let base_align = std::mem::align_of::<S>();
    let min_align = if cfg!(target_arch = "x86_64") {
        32usize
    } else {
        1usize
    };

    if base_align > min_align {
        base_align
    } else {
        min_align
    }
}

/// A continuous buffer in the "raster order".
///
/// The buffer is aligned so that it can be used in SIMD instructions.
#[derive(Debug, Clone)]
pub struct SimpleGrid<S> {
    width: usize,
    height: usize,
    offset: usize,
    buf: Vec<S>,
}

impl<S: Default + Clone> SimpleGrid<S> {
    const ALIGN: usize = compute_align::<S>();

    /// Create a new buffer.
    pub fn new(width: usize, height: usize) -> Self {
        let len = width * height;
        let mut buf = vec![S::default(); len];

        let extra = buf.as_ptr() as usize & (Self::ALIGN - 1);
        let offset = (Self::ALIGN - extra) % Self::ALIGN;
        buf.resize(buf.len() + offset, S::default());
        Self {
            width,
            height,
            offset,
            buf,
        }
    }
}

impl<S> SimpleGrid<S> {
    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Option<&S> {
        if x >= self.width || y >= self.height {
            return None;
        }

        Some(&self.buf[y * self.width + x + self.offset])
    }

    #[inline]
    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut S> {
        if x >= self.width || y >= self.height {
            return None;
        }

        Some(&mut self.buf[y * self.width + x + self.offset])
    }

    /// Get the immutable slice to the underlying buffer.
    #[inline]
    pub fn buf(&self) -> &[S] {
        &self.buf[self.offset..]
    }

    /// Get the mutable slice to the underlying buffer.
    #[inline]
    pub fn buf_mut(&mut self) -> &mut [S] {
        &mut self.buf[self.offset..]
    }

    #[inline]
    pub(crate) fn into_buf_iter(self) -> impl Iterator<Item = S> {
        self.buf.into_iter().skip(self.offset)
    }
}

/// A mutable subgrid of the underlying buffer.
#[derive(Debug)]
pub struct CutGrid<'g, Lane: Copy = f32> {
    ptr: NonNull<Lane>,
    width: usize,
    height: usize,
    stride: usize,
    _marker: std::marker::PhantomData<&'g mut [Lane]>
}

impl<'g, Lane: Copy> CutGrid<'g, Lane> {
    /// Create a `CutGrid` from raw pointer to the buffer, width, height and stride.
    ///
    /// # Safety
    /// The area specified by `width`, `height` and `stride` must not overlap with other instances
    /// of `CutGrid`, and the memory region in the area must be valid.
    pub unsafe fn new(ptr: NonNull<Lane>, width: usize, height: usize, stride: usize) -> Self {
        Self {
            ptr,
            width,
            height,
            stride,
            _marker: Default::default(),
        }
    }

    /// Create a `CutGrid` from buffer slice, width, height and stride.
    ///
    /// # Panic
    /// Panics if:
    /// - either `width` or `height` is zero,
    /// - `width` is greater than `stride`,
    /// - or the area specified by `width`, `height` and `stride` is larger than `buf`.
    pub fn from_buf(buf: &'g mut [Lane], width: usize, height: usize, stride: usize) -> Self {
        assert!(width > 0);
        assert!(height > 0);
        assert!(width <= stride);
        assert!(buf.len() >= stride * (height - 1) + width);
        // SAFETY: We have unique access to `buf`, and the area is in bounds.
        unsafe {
            Self::new(NonNull::new(buf.as_mut_ptr()).unwrap(), width, height, stride)
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    fn get_ptr(&self, x: usize, y: usize) -> *mut Lane {
        if x >= self.width || y >= self.height {
            panic!("Coordinate out of range: ({}, {}) not in {}x{}", x, y, self.width, self.height);
        }

        // SAFETY: (x, y) is checked above and is in bounds.
        unsafe {
            let offset = y * self.stride + x;
            self.ptr.as_ptr().add(offset)
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Lane {
        let ptr = self.get_ptr(x, y);
        // SAFETY: get_ptr returns a valid pointer.
        unsafe { *ptr }
    }

    #[inline]
    pub fn get_row(&self, row: usize) -> &[Lane] {
        let ptr = self.get_ptr(0, row);
        unsafe { std::slice::from_raw_parts(ptr as *const _, self.width) }
    }

    #[inline]
    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut Lane {
        let ptr = self.get_ptr(x, y);
        // SAFETY: get_ptr returns a valid pointer, and mutable borrow of `self` makes sure that
        // the access is exclusive.
        unsafe { ptr.as_mut().unwrap() }
    }

    #[inline]
    pub fn get_row_mut(&mut self, row: usize) -> &mut [Lane] {
        let ptr = self.get_ptr(0, row);
        unsafe { std::slice::from_raw_parts_mut(ptr, self.width) }
    }

    #[inline]
    pub fn swap(&mut self, (ax, ay): (usize, usize), (bx, by): (usize, usize)) {
        let a = self.get_ptr(ax, ay);
        let b = self.get_ptr(bx, by);
        if std::ptr::eq(a, b) {
            return;
        }

        // SAFETY: `a` and `b` are valid and aligned.
        unsafe {
            std::ptr::swap(a, b);
        }
    }
}

impl<'g, Lane: SimdLane> CutGrid<'g, Lane> {
    pub fn convert_grid(grid: &'g mut CutGrid<'_, f32>) -> Option<Self> {
        let mask = Lane::SIZE - 1;
        let align_mask = std::mem::align_of::<Lane>() - 1;

        (
            grid.ptr.as_ptr() as usize & align_mask == 0 &&
            grid.width & mask == 0 &&
            grid.stride & mask == 0
        ).then(|| Self {
            ptr: grid.ptr.cast::<Lane>(),
            width: grid.width / Lane::SIZE,
            height: grid.height,
            stride: grid.stride / Lane::SIZE,
            _marker: Default::default(),
        })
    }
}