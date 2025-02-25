//!
//! アセンブリを記述したモジュール
//!

use core::arch::asm;

pub fn get_currentel() -> u64 {
    let currentel: u64;
    unsafe { asm!("mrs {}, currentel", out(reg) currentel) };
    currentel
}

pub unsafe fn set_hcr_el2(hcr_el2: u64) {
    unsafe { asm!("msr hcr_el2, {}", in(reg) hcr_el2) };
}

pub unsafe fn set_elr_el2(elr_el2: u64) {
    unsafe { asm!("msr elr_el2, {}", in(reg) elr_el2) };
}

pub unsafe fn set_spsr_el2(spsr_el2: u64) {
    unsafe { asm!("msr spsr_el2, {}", in(reg) spsr_el2) };
}

pub unsafe fn eret() -> ! {
    unsafe { asm!("eret", options(noreturn)) }
}
