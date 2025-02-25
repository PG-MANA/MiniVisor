#![no_std]
#![no_main]

#[macro_use]
mod serial;
mod asm;
mod dtb;
mod drivers {
    pub mod pl011;
}
mod registers;

use registers::*;

use core::arch::asm;
use core::ffi::CStr;
use core::mem::MaybeUninit;
use core::slice;

/// グローバル変数置き場
static mut PL011_DEVICE: MaybeUninit<drivers::pl011::Pl011> = MaybeUninit::uninit();

#[unsafe(no_mangle)]
extern "C" fn main(argc: usize, argv: *const *const u8) -> usize {
    if argc != 1 {
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
