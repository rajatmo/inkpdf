//! Vertex buffers that grow when they must and are otherwise written only when content changes.

/// A vertex buffer that grows to the next power of two when its content no longer fits.
pub(super) struct GrowBuffer {
    buffer: wgpu::Buffer,
    capacity: u64,
    label: &'static str,
}

impl GrowBuffer {
    pub fn new(device: &wgpu::Device, label: &'static str, capacity: u64) -> Self {
        Self { buffer: create(device, label, capacity), capacity, label }
    }

    /// Make room for `bytes`. Growing discards what was in the buffer: callers rewrite it whole.
    pub fn reserve(&mut self, device: &wgpu::Device, bytes: u64) {
        if bytes > self.capacity {
            self.capacity = bytes.next_power_of_two();
            self.buffer = create(device, self.label, self.capacity);
        }
    }

    pub fn write(&self, queue: &wgpu::Queue, offset: u64, bytes: &[u8]) {
        if !bytes.is_empty() {
            queue.write_buffer(&self.buffer, offset, bytes);
        }
    }

    pub fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(..)
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }
}

fn create(device: &wgpu::Device, label: &'static str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Instances (capsules, panels) kept between frames and rewritten only when they change.
pub(super) struct InstanceBuffer {
    buffer: GrowBuffer,
    len: u32,
}

impl InstanceBuffer {
    /// Room for `count` instances of `T` to start with.
    pub fn new<T>(device: &wgpu::Device, label: &'static str, count: u64) -> Self {
        Self { buffer: GrowBuffer::new(device, label, count * std::mem::size_of::<T>() as u64), len: 0 }
    }

    pub fn set<T: bytemuck::Pod>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, items: &[T]) {
        self.buffer.reserve(device, std::mem::size_of_val(items) as u64);
        self.buffer.write(queue, 0, bytemuck::cast_slice(items));
        self.len = items.len() as u32;
    }

    /// Add `items` after the ones already there. `false`, doing nothing, when they do not fit: the
    /// caller then writes everything again with [`Self::set`], which makes room.
    pub fn append<T: bytemuck::Pod>(&mut self, queue: &wgpu::Queue, items: &[T]) -> bool {
        let size = std::mem::size_of::<T>() as u64;
        if (u64::from(self.len) + items.len() as u64) * size > self.buffer.capacity() {
            return false;
        }
        self.buffer.write(queue, u64::from(self.len) * size, bytemuck::cast_slice(items));
        self.len += items.len() as u32;
        true
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice()
    }
}
