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

pub fn get_stack_pointer() -> u64 {
    let sp: u64;
    unsafe { asm!("mov {}, sp", out(reg) sp) };
    sp
}

pub fn get_id_aa64mmfr0_el1() -> u64 {
    let id_aa64mmfr0_el1: u64;
    unsafe { asm!("mrs {}, id_aa64mmfr0_el1", out(reg) id_aa64mmfr0_el1) };
    id_aa64mmfr0_el1
}

pub fn get_vtcr_el2() -> u64 {
    let vtcr_el2: u64;
    unsafe { asm!("mrs {}, vtcr_el2", out(reg) vtcr_el2) };
    vtcr_el2
}

pub unsafe fn set_vtcr_el2(vtcr_el2: u64) {
    unsafe { asm!("msr vtcr_el2, {}", in(reg) vtcr_el2) };
}

pub fn get_vttbr_el2() -> u64 {
    let vttbr_el2: u64;
    unsafe { asm!("mrs {}, vttbr_el2", out(reg) vttbr_el2) };
    vttbr_el2
}

pub unsafe fn set_vttbr_el2(vttbr_el2: u64) {
    unsafe { asm!("msr vttbr_el2, {}", in(reg) vttbr_el2) };
}

pub fn flush_tlb_el1() {
    unsafe {
        asm!(
            "
            dsb ishst
            tlbi alle1is
            "
        );
    }
}
