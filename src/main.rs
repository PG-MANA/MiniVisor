#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
mod serial;
mod asm;
mod dtb;
mod drivers {
    pub mod generic_timer;
    pub mod gicv3;
    pub mod pl011;
    pub mod virtio;
    pub mod virtio_blk;
}
mod elf;
mod exception;
mod fat32;
mod lock;
mod memory_allocator;
mod mmio {
    pub mod gicv3;
    pub mod pl011;
    pub mod virtio_blk;
}
mod paging;
mod psci;
mod registers;
mod vgic;
mod vm;

use drivers::{generic_timer, gicv3, pl011, virtio_blk};
use lock::Mutex;

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::CStr;
use core::mem::MaybeUninit;
use core::slice;

struct GlobalAllocator {}

/// グローバル変数置き場
static PL011_DEVICE: Mutex<pl011::Pl011> = Mutex::new(pl011::Pl011::invalid());
static mut PL011_INT_ID: u32 = 0;
static MEMORY_ALLOCATOR: Mutex<memory_allocator::MemoryAllocator> =
    Mutex::new(memory_allocator::MemoryAllocator::new());
static VIRTIO_BLK: Mutex<virtio_blk::VirtioBlk> = Mutex::new(virtio_blk::VirtioBlk::invalid());
static mut FAT32: MaybeUninit<fat32::Fat32> = MaybeUninit::uninit();
#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator {};

/// 定数
const STACK_SIZE: usize = 0x10000;

#[unsafe(no_mangle)]
extern "C" fn main(argc: usize, argv: *const *const u8) -> usize {
    let stack_pointer = asm::get_stack_pointer() as usize;
    if argc != 2 {
        return 1;
    }
    let args = unsafe { slice::from_raw_parts(argv, argc) };
    /* argv[0] は DTB */
    let Ok(arg_0) = unsafe { CStr::from_ptr(args[0]) }.to_str() else {
        /* 変換に失敗 */
        return 2;
    };
    let Some(dtb_address) = str_to_usize(arg_0) else {
        return 3;
    };
    let Ok(dtb) = dtb::Dtb::new(dtb_address) else {
        return 4;
    };
    if let Err(e) = init_serial_port(&dtb) {
        return e;
    }

    println!("Hello, world!");

    let current_el = asm::get_currentel() >> 2;
    println!("CurrentEL: {}", current_el);
    assert_eq!(current_el, 2);

    /* メモリ管理のセットアップ */
    /* argv[1] は ELFヘッダの位置 */
    let arg_1 = unsafe { CStr::from_ptr(args[1]) }
        .to_str()
        .expect("Failed to get argv[1]");
    let elf_address = str_to_usize(arg_1).expect("Failed to convert the address");
    setup_memory(&dtb, dtb_address, elf_address, stack_pointer);

    exception::setup_exception();
    let distributor = init_gic_distributor(&dtb);
    let redistributor = init_gic_redistributor(&dtb);

    enable_serial_port_interrupt(&PL011_DEVICE.lock(), &distributor);

    generic_timer::init_generic_timer_global(&dtb);

    let mut virtblk = init_virtio_blk(&dtb).unwrap();
    let fat32 = init_fat32(&mut virtblk);

    let (boot_address, argument) = vm::create_vm(&fat32, &mut virtblk, &redistributor);

    *VIRTIO_BLK.lock() = virtblk;
    unsafe { (&raw mut FAT32).as_mut().unwrap().write(fat32) };

    /* PSCIのバージョンチェック */
    let (major_version, minor_version) = psci::check_psci_version().expect("PSCI is not supported");
    println!("PSCI version {major_version}.{minor_version}");

    launch_cpu(&dtb);

    unimplemented!();

    vm::boot_vm(boot_address, argument)
}

fn str_to_usize(s: &str) -> Option<usize> {
    let radix;
    let start;
    match s.get(0..2) {
        Some("0x") => {
            radix = 16;
            start = s.get(2..);
        }
        Some("0o") => {
            radix = 8;
            start = s.get(2..);
        }
        Some("0b") => {
            radix = 2;
            start = s.get(2..);
        }
        _ => {
            radix = 10;
            start = Some(s);
        }
    }
    usize::from_str_radix(start?, radix).ok()
}

fn init_serial_port(dtb: &dtb::Dtb) -> Result<(), usize> {
    let mut pl011 = None;
    loop {
        pl011 = dtb.search_node_by_compatible(b"arm,pl011", pl011.as_ref());
        match &pl011 {
            Some(d) => {
                if !dtb.is_node_operational(d) {
                    continue;
                } else {
                    break;
                }
            }
            None => {
                return Err(5);
            }
        }
    }
    let pl011 = pl011.unwrap();
    let Some((pl011_base, pl011_range)) = dtb.read_reg_property(&pl011, 0) else {
        return Err(6);
    };

    let interrupts =
        dtb.read_property_as_u32_array(&dtb.get_property(&pl011, b"interrupts").unwrap());
    let mut interrupt_number = 0;
    if u32::from_be(interrupts[0]) == gicv3::DTB_GIC_SPI
        && u32::from_be(interrupts[2]) == gicv3::DTB_GIC_LEVEL
    {
        interrupt_number = gicv3::GIC_SPI_BASE + u32::from_be(interrupts[1]);
    }

    let Ok(pl011) = pl011::Pl011::new(pl011_base, pl011_range) else {
        return Err(7);
    };
    unsafe { PL011_INT_ID = interrupt_number };
    *PL011_DEVICE.lock() = pl011;
    serial::init_default_serial_port(&PL011_DEVICE);
    Ok(())
}

#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{info}");
    loop {
        core::hint::spin_loop();
    }
}

pub fn setup_memory(dtb: &dtb::Dtb, dtb_address: usize, elf_address: usize, stack_pointer: usize) {
    let memory = dtb
        .search_node(b"memory", None)
        .expect("Expected memory node.");
    let (start, size) = dtb
        .read_reg_property(&memory, 0)
        .expect("Expected reg entry");
    println!("RAM is [{:#X} ~ {:#X}]", start, start + size);
    let mut memory_allocator = MEMORY_ALLOCATOR.lock();
    memory_allocator
        .free(start, size)
        .expect("Failed to free the RAM");

    /* DTBを除外 */
    println!(
        "DTB is [{:#X} ~ {:#X}]",
        dtb_address,
        dtb_address + dtb.get_total_size()
    );
    memory_allocator
        .reserve_memory(dtb_address, dtb.get_total_size(), 0)
        .expect("Failed to reserve DTB");

    let elf_header = elf::Elf64Header::new(elf_address).expect("Invalid ELF Header");
    for p in elf_header.get_program_headers() {
        if p.get_segment_type() == elf::ELF_PROGRAM_HEADER_SEGMENT_LOAD {
            println!(
                "Reserve [{:#X} ~ {:#X}]",
                p.get_physical_address(),
                p.get_physical_address() + p.get_memory_size()
            );
            memory_allocator
                .reserve_memory(
                    p.get_physical_address() as usize,
                    p.get_memory_size() as usize,
                    0,
                )
                .expect("Failed to reserve memory for the segment");
        }
    }

    /* Stackを除外 */
    let stack_end = ((stack_pointer - 1) & !(paging::PAGE_SIZE - 1)) + paging::PAGE_SIZE;
    let stack_start = stack_end - STACK_SIZE;
    println!("Reserve [{:#X} ~ {:#X}] for Stack", stack_start, stack_end);
    memory_allocator
        .reserve_memory(stack_start, STACK_SIZE, 0)
        .expect("Failed to reserve memory for the stack");
}

pub fn allocate_pages(
    number_of_pages: usize,
    align: usize,
) -> Result<usize, memory_allocator::MemoryError> {
    match MEMORY_ALLOCATOR
        .lock()
        .allocate(number_of_pages << paging::PAGE_SHIFT, align)
    {
        Ok(a) => Ok(a),
        Err(e) => {
            println!("Failed to allocate memory: {:?}", e);
            Err(e)
        }
    }
}

pub fn free_pages(address: usize, number_of_pages: usize) {
    let _ = MEMORY_ALLOCATOR
        .lock()
        .free(address, number_of_pages << paging::PAGE_SHIFT);
}

fn init_gic_distributor(dtb: &dtb::Dtb) -> gicv3::GicDistributor {
    let gic_node = dtb.search_node_by_compatible(b"arm,gic-v3", None).unwrap();
    let (base_address, size) = dtb.read_reg_property(&gic_node, 0).unwrap();
    println!("GIC Distributor's Base Address: {:#X}", base_address);
    let gic_distributor = gicv3::GicDistributor::new(base_address, size).unwrap();
    gic_distributor.init();
    gic_distributor
}

fn init_gic_redistributor(dtb: &dtb::Dtb) -> gicv3::GicRedistributor {
    let gic_node = dtb.search_node_by_compatible(b"arm,gic-v3", None).unwrap();
    let (base_address, size) = dtb.read_reg_property(&gic_node, 1).unwrap();
    println!("GIC Redistributor's Base Address: {:#X}", base_address);
    let gic_redistributor = gicv3::get_self_redistributor(base_address, size).unwrap();
    gic_redistributor.init();
    gic_redistributor
}

fn enable_serial_port_interrupt(pl011: &pl011::Pl011, distributor: &gicv3::GicDistributor) {
    let int_id = unsafe { PL011_INT_ID };
    if int_id == 0 {
        println!("PL011 does not support interrupt.");
        return;
    }
    distributor.set_group(int_id, gicv3::GicGroup::NonSecureGroup1);
    distributor.set_priority(int_id, 0x00);
    distributor.set_routing(int_id, false, asm::get_mpidr_el1());
    distributor.set_trigger_mode(int_id, true);
    distributor.set_pending(int_id, false);
    distributor.set_enable(int_id, true);
    pl011.enable_interrupt();
}

fn init_virtio_blk(dtb: &dtb::Dtb) -> Option<virtio_blk::VirtioBlk> {
    let mut virtio = None;
    loop {
        virtio = dtb.search_node_by_compatible(b"virtio,mmio", virtio.as_ref());
        match &virtio {
            Some(virtio) => {
                if dtb.is_node_operational(virtio) {
                    let (base_address, _) = dtb.read_reg_property(virtio, 0).unwrap();
                    if let Ok(blk) = virtio_blk::VirtioBlk::new(base_address) {
                        return Some(blk);
                    }
                }
            }
            None => {
                return None;
            }
        }
    }
}

pub fn init_fat32(blk: &mut virtio_blk::VirtioBlk) -> fat32::Fat32 {
    #[repr(C)]
    struct PartitionTableEntry {
        boot_flag: u8,
        first_sector: [u8; 3],
        partition_type: u8,
        last_sector: [u8; 3],
        first_sector_lba: u32,
        number_of_sectors: u32,
    }
    const PARTITION_TABLE_BASE: usize = 0x1BE;
    /* MBRの読み込み */
    let mut mbr: [u8; 512] = [0; 512];
    blk.read(&mut mbr as *mut _ as usize, 0, 512)
        .expect("Failed to read first 512bytes");
    /* BOOT Signatureの確認 */
    assert_eq!(u16::from_le_bytes([mbr[510], mbr[511]]), 0xAA55);

    /* パーテイションテーブルの解析 */
    let partition_table = unsafe {
        &*(&mbr[PARTITION_TABLE_BASE] as *const _ as usize as *const [PartitionTableEntry; 4])
    };
    let mut fat32 = Err(());
    for e in partition_table {
        if e.partition_type == 0x0C {
            fat32 = fat32::Fat32::new(blk, e.first_sector_lba as usize, 512);
            break;
        }
    }

    let fat32 = fat32.expect("The FAT32 Partition is not found!");
    fat32.list_files();

    fat32
}

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match MEMORY_ALLOCATOR
            .lock()
            .allocate(layout.size(), layout.align())
        {
            Ok(address) => address as *mut u8,
            Err(e) => {
                println!("Failed to allocate memory: {:?}", e);
                core::ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = MEMORY_ALLOCATOR.lock().free(ptr as usize, layout.size());
    }
}

pub fn launch_cpu(dtb: &dtb::Dtb) {
    let mut cpu_node = None;
    let current_affinity = asm::mpidr_to_affinity(asm::get_mpidr_el1());
    while let Some(cpu) = dtb.search_node(b"cpu", cpu_node.as_ref()) {
        if let Some((affinity, _)) = dtb.read_reg_property(&cpu, 0)
            && current_affinity != affinity as u64
        {
            println!("CPU_ON: {:#X}", affinity);
            let stack_address = allocate_pages(STACK_SIZE >> paging::PAGE_SHIFT, 0)
                .expect("Failed to allocate memory")
                + STACK_SIZE;
            if let Err(e) = psci::cpu_on(
                affinity as u64,
                asm::core_entry as *const fn() as usize as u64,
                stack_address as u64,
            ) {
                println!("Failed to start CPU(Affinity: {:#X}): {:?}", affinity, e);
                free_pages(stack_address - STACK_SIZE, STACK_SIZE >> paging::PAGE_SHIFT);
            }
        }
        cpu_node = Some(cpu);
    }
}

extern "C" fn core_main() -> ! {
    println!(
        "Hello, world! from {:#X}",
        asm::mpidr_to_affinity(asm::get_mpidr_el1())
    );
    loop {}
}
