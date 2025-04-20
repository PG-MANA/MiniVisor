//!
//! VirtIO 共通の実装
//!

pub const VIRTIO_MMIO_MAGIC: usize = 0x000;
pub const VIRTIO_MMIO_MAGIC_VALUE: u32 = 0x74726976;
pub const VIRTIO_MMIO_VERSION: usize = 0x04;
pub const VIRTIO_MMIO_DEVICE_ID: usize = 0x008;
pub const VIRTIO_MMIO_VENDOR_ID: usize = 0x00c;
pub const VIRTIO_MMIO_DEVICE_FEATURES: usize = 0x010;
pub const VIRTIO_MMIO_DRIVER_FEATURES: usize = 0x020;
pub const VIRTIO_MMIO_GUEST_PAGE_SIZE: usize = 0x028;
pub const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030;
pub const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034;
pub const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038;
pub const VIRTIO_MMIO_QUEUE_PFN: usize = 0x040;
pub const VIRTIO_MMIO_QUEUE_READY: usize = 0x044;
pub const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050;
pub const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060;
pub const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
pub const VIRTIO_MMIO_STATUS: usize = 0x070;

pub const VIRTIO_CONFIG_OFFSET: usize = 0x100;

pub const VIRTIO_DEVICE_STATUS_ACKNOWLEDGE: u32 = 1;
pub const VIRTIO_DEVICE_STATUS_DRIVER: u32 = 2;
pub const VIRTIO_DEVICE_STATUS_DRIVER_OK: u32 = 4;
pub const VIRTIO_DEVICE_STATUS_FEATURES_OK: u32 = 8;

#[repr(C)]
pub struct VirtQueueDesc {
    pub address: u64,
    pub length: u32,
    pub flags: u16,
    pub next: u16,
}

pub const VIRT_QUEUE_DESC_FLAGS_NEXT: u16 = 1;
pub const VIRT_QUEUE_DESC_FLAGS_WRITE: u16 = 1 << 1;

pub const NUMBER_OF_DESCRIPTORS: usize = 64;
pub const VIRTIO_PAGE_SHIFT: usize = 12;
pub const VIRTIO_PAGE_SIZE: usize = 1 << VIRTIO_PAGE_SHIFT;

#[repr(C)]
pub struct VirtQueueAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; NUMBER_OF_DESCRIPTORS],
    pub used_event: u16,
}

#[repr(C)]
pub struct VirtQueueUsedElement {
    pub id: u32,
    pub length: u32, /* The number of bytes written into buffers */
}

#[repr(C)]
pub struct VirtQueueUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtQueueUsedElement; NUMBER_OF_DESCRIPTORS],
    pub avail_event: u16,
}

pub const NUMBER_OF_PAGES_QUEUE: usize = (((size_of::<VirtQueueDesc>() * NUMBER_OF_DESCRIPTORS
    + size_of::<VirtQueueAvail>())
    >> VIRTIO_PAGE_SHIFT)
    + 1)
    + ((size_of::<VirtQueueUsed>() >> VIRTIO_PAGE_SHIFT) + 1);
