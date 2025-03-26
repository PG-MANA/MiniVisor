//!
//! Virtio-Blkの実装
//!

use crate::drivers::virtio::*;

pub const VIRTIO_BLK_TYPE_IN: u32 = 0;
pub const VIRTIO_BLK_TYPE_OUT: u32 = 1;
pub const VIRTIO_BLK_S_OK: u8 = 0;
pub const VIRTIO_BLK_S_IOERR: u8 = 1;

/// ディスクがRead Onlyかどうか
const VIRTIO_BLK_F_RO: u32 = 1 << 5;

#[repr(C)]
pub struct VirtioBlkReq {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

pub struct VirtioBlk {
    base_address: usize,
    descriptors: *mut [VirtQueueDesc; NUMBER_OF_DESCRIPTORS],
    avail: *mut VirtQueueAvail,
    #[allow(dead_code)]
    used: *mut VirtQueueUsed,
    free_bitmap: [u8; NUMBER_OF_DESCRIPTORS / (u8::BITS as usize)],
}

impl VirtioBlk {
    pub fn new(base_address: usize) -> Result<Self, ()> {
        if Self::read_register(base_address, VIRTIO_MMIO_MAGIC) != VIRTIO_MMIO_MAGIC_VALUE {
            return Err(());
        }
        if Self::read_register(base_address, VIRTIO_MMIO_VERSION) != 1 {
            return Err(());
        }
        if Self::read_register(base_address, VIRTIO_MMIO_DEVICE_ID) != 2
            || Self::read_register(base_address, VIRTIO_MMIO_VENDOR_ID) != 0x554d4551
        {
            return Err(());
        }
        /* デバイスのリセット */
        Self::write_register(base_address, VIRTIO_MMIO_STATUS, 0);
        /* デバイスを認識した事を通知 */
        Self::write_register(
            base_address,
            VIRTIO_MMIO_STATUS,
            Self::read_register(base_address, VIRTIO_MMIO_STATUS)
                | VIRTIO_DEVICE_STATUS_ACKNOWLEDGE,
        );

        /* デバイスの対応機能を取得 */
        Self::write_register(
            base_address,
            VIRTIO_MMIO_STATUS,
            Self::read_register(base_address, VIRTIO_MMIO_STATUS) | VIRTIO_DEVICE_STATUS_DRIVER,
        );
        let mut features = Self::read_register(base_address, VIRTIO_MMIO_DEVICE_FEATURES);
        /* デバイスがReadOnlyかどうか確認 */
        if (features & VIRTIO_BLK_F_RO) != 0 {
            println!("Disk is readonly.");
            return Err(());
        }
        /* ドライバの対応状況を設定 */
        features = 0;
        Self::write_register(base_address, VIRTIO_MMIO_DRIVER_FEATURES, features);
        Self::write_register(
            base_address,
            VIRTIO_MMIO_STATUS,
            Self::read_register(base_address, VIRTIO_MMIO_STATUS)
                | VIRTIO_DEVICE_STATUS_FEATURES_OK,
        );

        /* VirtQueueの設定 */
        Self::write_register(
            base_address,
            VIRTIO_MMIO_GUEST_PAGE_SIZE,
            VIRTIO_PAGE_SIZE as u32,
        );
        Self::write_register(base_address, VIRTIO_MMIO_QUEUE_SEL, 0);
        let queue_max = Self::read_register(base_address, VIRTIO_MMIO_QUEUE_NUM_MAX);
        if (queue_max as usize) < NUMBER_OF_DESCRIPTORS {
            println!("Virtio Queue Size is invalid: {queue_max}");
            return Err(());
        }
        Self::write_register(
            base_address,
            VIRTIO_MMIO_QUEUE_NUM,
            NUMBER_OF_DESCRIPTORS as u32,
        );
        let queue = crate::allocate_pages(NUMBER_OF_PAGES_QUEUE, VIRTIO_PAGE_SHIFT)
            .expect("Failed to allocate virtio queue");
        unsafe {
            core::ptr::write_bytes(
                queue as *mut u8,
                0,
                NUMBER_OF_PAGES_QUEUE << VIRTIO_PAGE_SHIFT,
            )
        };
        Self::write_register(
            base_address,
            VIRTIO_MMIO_QUEUE_PFN,
            (queue >> VIRTIO_PAGE_SHIFT) as u32,
        );

        /* 設定完了を通知 */
        Self::write_register(
            base_address,
            VIRTIO_MMIO_STATUS,
            Self::read_register(base_address, VIRTIO_MMIO_STATUS) | VIRTIO_DEVICE_STATUS_DRIVER_OK,
        );

        /* VirtQueueの各要素のアドレス計算 */
        let descriptor_table = queue;
        let available_ring = descriptor_table + size_of::<VirtQueueDesc>() * NUMBER_OF_DESCRIPTORS;
        let used_ring = ((available_ring + size_of::<VirtQueueAvail>() - 1)
            & !(VIRTIO_PAGE_SIZE - 1))
            + VIRTIO_PAGE_SIZE;
        Ok(Self {
            base_address,
            descriptors: descriptor_table as *mut _,
            avail: available_ring as *mut _,
            used: used_ring as *mut _,
            free_bitmap: [u8::MAX; NUMBER_OF_DESCRIPTORS / (u8::BITS as usize)],
        })
    }

    fn read_register(base_address: usize, offset: usize) -> u32 {
        unsafe { core::ptr::read_volatile((base_address + offset) as *const u32) }
    }

    fn write_register(base_address: usize, offset: usize, data: u32) {
        unsafe { core::ptr::write_volatile((base_address + offset) as *mut u32, data) }
    }

    fn allocate_descriptor(&mut self) -> Option<(u16, &'static mut VirtQueueDesc)> {
        for (byte, c) in self.free_bitmap.iter_mut().enumerate() {
            for bit in 0..(u8::BITS as usize) {
                if (*c & (1 << bit)) != 0 {
                    *c &= !(1 << bit);
                    let index = (byte * u8::BITS as usize) + bit;
                    return Some((index as u16, &mut unsafe { &mut *self.descriptors }[index]));
                }
            }
        }
        None
    }

    fn free_descriptor(&mut self, index: u16) {
        assert!((index as usize) < NUMBER_OF_DESCRIPTORS);
        self.free_bitmap[(index as usize) / (u8::BITS as usize)] |=
            1 << (index & ((1 << u8::BITS.ilog2() as u16) - 1));
    }

    fn operation_sync(
        &mut self,
        buffer_address: usize,
        block_address: u64,
        length: u64,
        is_write: bool,
    ) -> Result<(), ()> {
        if (block_address & ((1 << 9) - 1)) != 0 || (length & ((1 << 9) - 1) != 0) {
            println!(
                "Block Address({:#X}) and Length({:#X}) must be 512Byte-Aligned.",
                block_address, length
            );
            return Err(());
        }

        /* Virtio BLK Requestの設定 */
        let virtio_blk_req = VirtioBlkReq {
            req_type: if is_write {
                VIRTIO_BLK_TYPE_OUT
            } else {
                VIRTIO_BLK_TYPE_IN
            },
            reserved: 0,
            sector: (block_address >> 9),
        };
        let Some((first_idx, first_descriptor)) = self.allocate_descriptor() else {
            println!("Failed to allocate descriptor");
            return Err(());
        };
        first_descriptor.address = &virtio_blk_req as *const _ as usize as u64;
        first_descriptor.length = size_of::<VirtioBlkReq>() as u32;
        first_descriptor.flags = VIRT_QUEUE_DESC_FLAGS_NEXT;

        /* Bufferの設定 */
        let Some((second_idx, second_descriptor)) = self.allocate_descriptor() else {
            println!("Failed to allocate descriptor");
            return Err(());
        };

        second_descriptor.address = buffer_address as _;
        second_descriptor.length = length as _;
        second_descriptor.flags = VIRT_QUEUE_DESC_FLAGS_NEXT;
        if !is_write {
            second_descriptor.flags |= VIRT_QUEUE_DESC_FLAGS_WRITE;
        }
        first_descriptor.next = second_idx;

        /* Statusの設定 */
        let mut status: u8 = 0xFF;
        let Some((third_idx, third_descriptor)) = self.allocate_descriptor() else {
            println!("Failed to allocate descriptor");
            return Err(());
        };
        third_descriptor.address = &mut status as *mut _ as usize as u64;
        third_descriptor.length = size_of::<u8>() as u32;
        third_descriptor.flags = VIRT_QUEUE_DESC_FLAGS_WRITE;
        second_descriptor.next = third_idx;

        /* Available Ring の更新 */
        let avail_ring = unsafe { &mut *self.avail };
        let idx = avail_ring.idx as usize;
        avail_ring.ring[idx % NUMBER_OF_DESCRIPTORS] = first_idx;
        avail_ring.idx += 1;

        /* デバイスに通知 */
        Self::write_register(self.base_address, VIRTIO_MMIO_QUEUE_NOTIFY, 0);

        /* Spin Wait */
        let result;
        loop {
            unsafe { crate::asm::invalidate_cache(&status as *const _ as usize) };
            let s = unsafe { core::ptr::read_volatile(&status) };
            if s != 0xFF {
                if s == VIRTIO_BLK_S_OK {
                    result = Ok(());
                } else {
                    result = Err(());
                }
                break;
            }
            core::hint::spin_loop();
        }
        self.free_descriptor(first_idx);
        self.free_descriptor(second_idx);
        self.free_descriptor(third_idx);

        result
    }

    pub fn read(
        &mut self,
        buffer_address: usize,
        block_address: u64,
        length: u64,
    ) -> Result<(), ()> {
        self.operation_sync(buffer_address, block_address, length, false)
    }

    pub fn write(
        &mut self,
        buffer_address: usize,
        block_address: u64,
        length: u64,
    ) -> Result<(), ()> {
        self.operation_sync(buffer_address, block_address, length, true)
    }
}
