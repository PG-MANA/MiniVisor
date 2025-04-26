//!
//! Virtio-Blk MMIO Driver
//!

use crate::drivers::{virtio::*, virtio_blk::*};
use crate::fat32::FileInfo;
use crate::vm::*;
use crate::{FAT32, VIRTIO_BLK};

use core::ptr::{null_mut, read_volatile, write_volatile};

const VIRTIO_BLK_INT_ID: u32 = 40;

pub struct VirtioBlkMmio {
    file: FileInfo,
    interrupt_status: u32,
    status: u32,
    queue_size: usize,
    queue_ready: bool,
    page_size: usize,
    descriptor: *mut VirtQueueDesc,
    avail_ring: *mut VirtQueueAvail,
    used_ring: *mut VirtQueueUsed,
    last_avail_id: u16,
    used_id: u16,
}

impl VirtioBlkMmio {
    pub fn new(file: FileInfo) -> Self {
        if (file.get_file_size() & 0x1FF) != 0 {
            panic!(
                "File Size must be 512-Byte aligned(Size: {:#X})",
                file.get_file_size()
            );
        }
        Self {
            file,
            interrupt_status: 0,
            status: 0,
            queue_size: 0,
            queue_ready: false,
            page_size: 1 << 12,
            descriptor: null_mut(),
            avail_ring: null_mut(),
            used_ring: null_mut(),
            last_avail_id: 0,
            used_id: 0,
        }
    }

    fn get_descriptor(&self, id: u16) -> Option<VirtQueueDesc> {
        if !self.descriptor.is_null() {
            Some(unsafe {
                read_volatile(
                    (self.descriptor as usize + size_of::<VirtQueueDesc>() * (id as usize))
                        as *const VirtQueueDesc,
                )
            })
        } else {
            None
        }
    }

    fn get_descriptor_id(&self, id: u16) -> Option<u16> {
        if !self.avail_ring.is_null() {
            Some(unsafe {
                read_volatile(
                    (self.avail_ring as usize
                        + size_of::<u16>() * 2 /* flag + idx */
                        + size_of::<u16>() * (id as usize)) as *const u16,
                )
            })
        } else {
            None
        }
    }

    fn get_next_avail_id(&mut self) -> Option<u16> {
        if self.last_avail_id == unsafe { &*self.avail_ring }.idx {
            return None;
        }
        let next = self.last_avail_id % (self.queue_size as u16);
        self.last_avail_id += 1;
        Some(next)
    }

    fn write_used(&mut self, id: u16, length: u32) {
        let used_id = self.used_id % (self.queue_size as u16);
        self.used_id += 1;
        unsafe {
            write_volatile(
                (self.used_ring as usize
                    + size_of::<u16>() * 2 /* flag + idx */
                    + size_of::<VirtQueueUsedElement>() * (used_id as usize))
                    as *mut VirtQueueUsedElement,
                VirtQueueUsedElement {
                    id: (id as u32),
                    length,
                },
            )
        };
        unsafe { &mut *self.used_ring }.idx = self.used_id;
    }

    fn operation(&mut self) {
        while let Some(id) = self.get_next_avail_id() {
            let Some(descriptor_id) = self.get_descriptor_id(id) else {
                println!("Failed to get the next descriptor id");
                return;
            };
            let Some(request_descriptor) = self.get_descriptor(descriptor_id) else {
                println!("Failed to get the next descriptor");
                return;
            };
            if request_descriptor.length as usize != size_of::<VirtioBlkReq>() {
                println!("Invalid VirtioBlkReq size");
                return;
            }
            let Some(blk_req) =
                get_current_vm().get_physical_address(request_descriptor.address as usize)
            else {
                println!("Invalid VirtioBlkReq address");
                return;
            };

            /* リクエストの解析 */
            let blk_req = unsafe { &*(blk_req as *const VirtioBlkReq) };
            let is_write = blk_req.req_type == VIRTIO_BLK_TYPE_OUT;
            let mut offset = (blk_req.sector << 9) as usize;
            if (request_descriptor.flags & VIRT_QUEUE_DESC_FLAGS_NEXT) == 0 {
                println!("Invalid VirtioBlkReq");
                return;
            }

            /* イメージファイルの読み書き */
            let mut descriptor = request_descriptor;
            let mut total_size = 0;
            let mut status = VIRTIO_BLK_S_OK;
            loop {
                if let Some(d) = self.get_descriptor(descriptor.next) {
                    descriptor = d;
                    if (descriptor.flags & VIRT_QUEUE_DESC_FLAGS_NEXT) == 0 {
                        break;
                    }
                } else {
                    println!("Failed to get the next descriptor");
                    return;
                }
                let size = descriptor.length;
                total_size += size;
                let Some(address) =
                    get_current_vm().get_physical_address(descriptor.address as usize)
                else {
                    println!(
                        "Failed to convert {:#x} to the physical address",
                        descriptor.address
                    );
                    status = VIRTIO_BLK_S_IOERR;
                    continue;
                };
                let mut virtio_blk = VIRTIO_BLK.lock();
                let fat32 = unsafe { (&raw mut FAT32).as_mut().unwrap().assume_init_mut() };
                let result = if is_write {
                    fat32.write(&self.file, &mut virtio_blk, address, offset, size as usize)
                } else {
                    fat32.read(&self.file, &mut virtio_blk, address, offset, size as usize)
                };
                if result.is_err() {
                    println!(
                        "Failed to {} {:#x} bytes from {:#x} to {:#x}",
                        if is_write { "write" } else { "read" },
                        size,
                        offset,
                        address
                    );
                    status = VIRTIO_BLK_S_IOERR;
                }
                offset += size as usize;
            }
            if let Some(a) = get_current_vm().get_physical_address(descriptor.address as usize) {
                unsafe { write_volatile(a as *mut u8, status) };
                total_size += descriptor.length;
            } else {
                println!("Failed to write the status");
            }
            self.write_used(descriptor_id, total_size);
        }
        self.interrupt_status |= 1;
        get_current_vm()
            .get_gic_distributor_mmio()
            .lock()
            .trigger_interrupt(VIRTIO_BLK_INT_ID, None);
    }
}

impl MmioHandler for VirtioBlkMmio {
    fn read(&mut self, offset: usize, _access_width: u64) -> Result<u64, ()> {
        let mut value = 0u64;
        match offset {
            VIRTIO_MMIO_MAGIC => {
                value = VIRTIO_MMIO_MAGIC_VALUE as u64;
            }
            VIRTIO_MMIO_VERSION => {
                value = 0x01;
            }
            VIRTIO_MMIO_DEVICE_ID => {
                value = 0x02;
            }
            VIRTIO_MMIO_VENDOR_ID => {
                value = 0x554d4551;
            }
            VIRTIO_MMIO_DEVICE_FEATURES => {
                value = 0x00;
            }
            VIRTIO_MMIO_QUEUE_NUM_MAX => {
                value = 1024;
            }
            VIRTIO_MMIO_QUEUE_READY => {
                value = self.queue_ready as _;
            }
            VIRTIO_MMIO_QUEUE_PFN => {
                value = (self.descriptor as usize / self.page_size) as u64;
            }
            VIRTIO_MMIO_INTERRUPT_STATUS => {
                value = self.interrupt_status as u64;
            }
            VIRTIO_MMIO_STATUS => {
                value = self.status as u64;
            }
            _ if offset >= VIRTIO_CONFIG_OFFSET => {
                let config_offset = offset - VIRTIO_CONFIG_OFFSET;
                if (0..8).contains(&config_offset) {
                    /* ブロックデバイスのサイズ */
                    let capacity = self.file.get_file_size() >> 9;
                    value = (capacity >> (config_offset * 8)) as u64;
                }
            }
            _ => { /* Unimplemented */ }
        }
        Ok(value)
    }

    fn write(&mut self, offset: usize, _access_width: u64, value: u64) -> Result<(), ()> {
        match offset {
            VIRTIO_MMIO_GUEST_PAGE_SIZE => {
                self.page_size = value as usize;
            }
            VIRTIO_MMIO_QUEUE_NUM => {
                self.queue_size = value as usize;
                self.avail_ring = ((self.descriptor as usize)
                    + size_of::<VirtQueueDesc>() * self.queue_size)
                    as *mut _;
                self.used_ring =
                    ((((self.avail_ring as usize + size_of::<u16>() * (3 + self.queue_size)) - 1)
                        & !(self.page_size - 1))
                        + self.page_size) as *mut _;
            }
            VIRTIO_MMIO_QUEUE_PFN => {
                if let Some(address) =
                    get_current_vm().get_physical_address((value as usize) * self.page_size)
                {
                    self.descriptor = address as *mut _;
                    self.avail_ring = ((self.descriptor as usize)
                        + size_of::<VirtQueueDesc>() * self.queue_size)
                        as *mut _;
                    self.used_ring = ((((self.avail_ring as usize
                        + size_of::<u16>() * (3 + self.queue_size))
                        - 1)
                        & !(self.page_size - 1))
                        + self.page_size) as *mut _;
                }
            }
            VIRTIO_MMIO_QUEUE_NOTIFY => {
                if value == 0 {
                    self.operation();
                }
            }
            VIRTIO_MMIO_INTERRUPT_ACK => {
                self.interrupt_status &= !(value as u32);
            }
            VIRTIO_MMIO_STATUS => {
                if value == 0 {
                    self.queue_size = 0;
                    self.queue_ready = false;
                    self.page_size = 1 << 12;
                    self.interrupt_status = 0;
                    self.status = 0;
                    self.descriptor = null_mut();
                    self.avail_ring = null_mut();
                    self.used_ring = null_mut();
                    self.last_avail_id = 0;
                    self.used_id = 0;
                } else {
                    self.status = value as u32;
                }
            }
            _ => { /* Unimplemented */ }
        }
        Ok(())
    }
}

unsafe impl core::marker::Send for VirtioBlkMmio {}
