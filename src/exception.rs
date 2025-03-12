//!
//! 割り込み制御
//!
use crate::asm;
use crate::registers::*;

use core::arch::global_asm;

#[repr(C)]
pub struct Registers {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub x6: u64,
    pub x7: u64,
    pub x8: u64,
    pub x9: u64,
    pub x10: u64,
    pub x11: u64,
    pub x12: u64,
    pub x13: u64,
    pub x14: u64,
    pub x15: u64,
    pub x16: u64,
    pub x17: u64,
    pub x18: u64,
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64,
    padding: u64,
}

/* 例外テーブル */
global_asm!(
    "
.section .text
.balign 0x800
.size   exception_table, 0x800
.global exception_table
exception_table:

.balign 0x080
synchronous_current_el_stack_pointer_0:
    b   synchronous_current_el_stack_pointer_0

.balign 0x080
irq_current_el_stack_pointer_0:
    b   irq_current_el_stack_pointer_0

.balign 0x080
fiq_current_el_stack_pointer_0:
    b   fiq_current_el_stack_pointer_0

.balign 0x080
s_error_current_el_stack_pointer_0:
    b   s_error_current_el_stack_pointer_0

.balign 0x080
synchronous_current_el_stack_pointer_x:
    b   synchronous_current_el_stack_pointer_x

.balign 0x080
irq_current_el_stack_pointer_x:
    sub sp,   sp, #(8 * 32)
    stp x30, xzr, [sp, #( 15 * 16)]
    stp x28, x29, [sp, #( 14 * 16)]
    stp x26, x27, [sp, #( 13 * 16)]
    stp x24, x25, [sp, #( 12 * 16)]
    stp x22, x23, [sp, #( 11 * 16)]
    stp x20, x21, [sp, #( 10 * 16)]
    stp x18, x19, [sp, #(  9 * 16)]
    stp x16, x17, [sp, #(  8 * 16)]
    stp x14, x15, [sp, #(  7 * 16)]
    stp x12, x13, [sp, #(  6 * 16)]
    stp x10, x11, [sp, #(  5 * 16)]
    stp  x8,  x9, [sp, #(  4 * 16)]
    stp  x6,  x7, [sp, #(  3 * 16)]
    stp  x4,  x5, [sp, #(  2 * 16)]
    stp  x2,  x3, [sp, #(  1 * 16)]
    stp  x0,  x1, [sp, #(  0 * 16)]
    mov  x0, sp
    adr x30, exit_exception
    b   {irq_handler}

.balign 0x080
fiq_current_el_stack_pointer_x:
    b   fiq_current_el_stack_pointer_x

.balign 0x080
s_error_current_el_stack_pointer_x:
    b   s_error_current_el_stack_pointer_x

.balign 0x080
synchronous_lower_el_aarch64:
    sub sp,   sp, #(8 * 32)
    stp x30, xzr, [sp, #( 15 * 16)]
    stp x28, x29, [sp, #( 14 * 16)]
    stp x26, x27, [sp, #( 13 * 16)]
    stp x24, x25, [sp, #( 12 * 16)]
    stp x22, x23, [sp, #( 11 * 16)]
    stp x20, x21, [sp, #( 10 * 16)]
    stp x18, x19, [sp, #(  9 * 16)]
    stp x16, x17, [sp, #(  8 * 16)]
    stp x14, x15, [sp, #(  7 * 16)]
    stp x12, x13, [sp, #(  6 * 16)]
    stp x10, x11, [sp, #(  5 * 16)]
    stp  x8,  x9, [sp, #(  4 * 16)]
    stp  x6,  x7, [sp, #(  3 * 16)]
    stp  x4,  x5, [sp, #(  2 * 16)]
    stp  x2,  x3, [sp, #(  1 * 16)]
    stp  x0,  x1, [sp, #(  0 * 16)]
    mov  x0, sp
    adr x30, exit_exception
    b   {synchronous_handler}

.balign 0x080
irq_lower_el_aarch64:
    sub sp,   sp, #(8 * 32)
    stp x30, xzr, [sp, #( 15 * 16)]
    stp x28, x29, [sp, #( 14 * 16)]
    stp x26, x27, [sp, #( 13 * 16)]
    stp x24, x25, [sp, #( 12 * 16)]
    stp x22, x23, [sp, #( 11 * 16)]
    stp x20, x21, [sp, #( 10 * 16)]
    stp x18, x19, [sp, #(  9 * 16)]
    stp x16, x17, [sp, #(  8 * 16)]
    stp x14, x15, [sp, #(  7 * 16)]
    stp x12, x13, [sp, #(  6 * 16)]
    stp x10, x11, [sp, #(  5 * 16)]
    stp  x8,  x9, [sp, #(  4 * 16)]
    stp  x6,  x7, [sp, #(  3 * 16)]
    stp  x4,  x5, [sp, #(  2 * 16)]
    stp  x2,  x3, [sp, #(  1 * 16)]
    stp  x0,  x1, [sp, #(  0 * 16)]
    mov  x0, sp
    adr x30, exit_exception
    b   {irq_handler}

.balign 0x080
fiq_lower_el_aarch64:
    b   fiq_lower_el_aarch64

.balign 0x080
s_error_lower_el_aarch64:
    b   s_error_lower_el_aarch64

.balign 0x080
synchronous_lower_el_aarch32:
    b   synchronous_lower_el_aarch32

.balign 0x080
irq_lower_el_aarch32:
    b   irq_lower_el_aarch32

.balign 0x080
fiq_lower_el_aarch32:
    b   fiq_lower_el_aarch32

.balign 0x080
s_error_lower_el_aarch32:
    b   s_error_lower_el_aarch32

exit_exception:
    ldp x30, xzr, [sp, #( 15 * 16)]
    ldp x28, x29, [sp, #( 14 * 16)]
    ldp x26, x27, [sp, #( 13 * 16)]
    ldp x24, x25, [sp, #( 12 * 16)]
    ldp x22, x23, [sp, #( 11 * 16)]
    ldp x20, x21, [sp, #( 10 * 16)]
    ldp x18, x19, [sp, #(  9 * 16)]
    ldp x16, x17, [sp, #(  8 * 16)]
    ldp x14, x15, [sp, #(  7 * 16)]
    ldp x12, x13, [sp, #(  6 * 16)]
    ldp x10, x11, [sp, #(  5 * 16)]
    ldp  x8,  x9, [sp, #(  4 * 16)]
    ldp  x6,  x7, [sp, #(  3 * 16)]
    ldp  x4,  x5, [sp, #(  2 * 16)]
    ldp  x2,  x3, [sp, #(  1 * 16)]
    ldp  x0,  x1, [sp, #(  0 * 16)]
    add  sp,  sp, #(8 * 32)
    eret
",
irq_handler = sym irq_handler,
synchronous_handler = sym synchronous_handler,
);

pub fn setup_exception() {
    unsafe extern "C" {
        static exception_table: *const u8;
    }
    unsafe { asm::set_vbar_el2(&exception_table as *const _ as usize as u64) };
}

extern "C" fn synchronous_handler(registers: *mut Registers) {
    let esr_el2 = asm::get_esr_el2();
    let ec = esr_el2 & ESR_EL2_EC;
    match ec {
        ESR_EL2_EC_DATA_ABORT => data_abort_handler(unsafe { &mut *registers }, esr_el2),
        _ => {
            panic!("Unknown Exception: {}", ec >> ESR_EL2_EC_BITS_OFFSET);
        }
    }
}

fn data_abort_handler(registers: &mut Registers, esr_el2: u64) {
    if esr_el2 & ESR_EL2_ISS_ISV == 0 {
        panic!("Data Abort Info is not available.");
    }
    let is_64bit_register = (esr_el2 & ESR_EL2_ISS_SF) != 0;
    let access_width = match (esr_el2 & ESR_EL2_ISS_SAS) >> ESR_EL2_ISS_SAS_BITS_OFFSET {
        0b00 => 8,
        0b01 => 16,
        0b10 => 32,
        0b11 => 64,
        _ => unreachable!(),
    };
    let is_write_access = (esr_el2 & ESR_EL2_ISS_WNR) != 0;

    let register_number = ((esr_el2 & ESR_EL2_ISS_SRT) >> ESR_EL2_ISS_SRT_BITS_OFFSET) as usize;
    let register: &mut u64 =
        &mut unsafe { &mut *(registers as *mut _ as usize as *mut [u64; 32]) }[register_number];

    let address = (((asm::get_hpfar_el2() & HPFAR_EL2_FIPA) >> HPFAR_EL2_FIPA_BITS_OFFSET)
        << crate::paging::PAGE_SHIFT)
        | (asm::get_far_el2() & ((1 << crate::paging::PAGE_SHIFT) - 1));

    if (0x9000000..0x9001000).contains(&address) {
        /* PL011 */
        use crate::mmio::pl011;
        let offset = (address - 0x9000000) as usize;
        if is_write_access {
            let register_value = if is_64bit_register {
                *register
            } else {
                *register & (u32::MAX as u64)
            };
            pl011::mmio_write(offset, access_width, register_value).expect("Failed to handle MMIO");
        } else {
            *register = pl011::mmio_read(offset, access_width).expect("Failed to handle MMIO");
        }
    }
    unsafe { asm::advance_elr_el2() };
}

extern "C" fn irq_handler() {}
