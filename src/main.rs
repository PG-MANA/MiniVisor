#![no_std]
#![no_main]

#[macro_use]
mod serial;
mod asm;
mod dtb;
mod drivers {
    pub mod pl011;
}
mod elf;
mod memory_allocator;
mod registers;

use registers::*;

use core::arch::asm;
use core::ffi::CStr;
use core::mem::MaybeUninit;
use core::slice;

/// グローバル変数置き場
static mut PL011_DEVICE: MaybeUninit<drivers::pl011::Pl011> = MaybeUninit::uninit();
static mut MEMORY_ALLOCATOR: memory_allocator::MemoryAllocator =
    memory_allocator::MemoryAllocator::new();

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

    setup_hypervisor_registers();

    unsafe {
        /* EL1h で動作する */
        asm::set_spsr_el2(SPSR_EL2_M_EL1H);
        /* ジャンプ先のアドレス */
        asm::set_elr_el2(el1_main as *const fn() as usize as u64);
        /* eret で el1_main に */
        asm::eret();
    }
}

extern "C" fn el1_main() {
    loop {
        unsafe {
            asm!("wfi");
        }
    }
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
    let Ok(pl011) = drivers::pl011::Pl011::new(pl011_base, pl011_range) else {
        return Err(7);
    };
    unsafe { (&raw mut PL011_DEVICE).write(MaybeUninit::new(pl011)) };
    serial::init_default_serial_port(unsafe {
        (&raw mut PL011_DEVICE).as_ref().unwrap().assume_init_ref()
    });
    Ok(())
}

pub fn setup_hypervisor_registers() {
    /* HCR_EL2 */
    let hcr_el2 = HCR_EL2_RW | HCR_EL2_API;
    unsafe { asm::set_hcr_el2(hcr_el2) };
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
    let memory_allocator = unsafe { (&raw mut MEMORY_ALLOCATOR).as_mut().unwrap() };
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
    match unsafe { (&raw mut MEMORY_ALLOCATOR).as_mut().unwrap() }
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
    let _ = unsafe { (&raw mut MEMORY_ALLOCATOR).as_mut().unwrap() }
        .free(address, number_of_pages << paging::PAGE_SHIFT);
}
