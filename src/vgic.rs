//!
//! Virtual Generic Interrupt Controller
//!

use crate::asm;
use crate::drivers::gicv3::{GicGroup, GicRedistributor};
use crate::mmio::gicv3::INJECT_INTERRUPT_INT_ID;
use crate::vm;

pub const MAINTENANCE_INTERRUPT_INTID: u32 = 25;

const ICH_HCR_EL2_EN: u64 = 1 << 0;

const ICH_VTR_EL2_LIST_REGS: u64 = 0b11111;

const ICH_LRN_EL2_STATUS: u64 = 0b11 << 62;
const ICH_LRN_EL2_STATUS_INACTIVE: u64 = 0b00 << 62;
const ICH_LRN_EL2_STATUS_PENDING: u64 = 0b01 << 62;
const ICH_LRN_EL2_HW: u64 = 1 << 61;
const ICH_LRN_EL2_EOI: u64 = 1 << 41;
const ICH_LRN_EL2_VINTID: u64 = (1 << 32) - 1;

const GET_ICH_LRN_EL2: [fn() -> u64; 3] = [
    asm::get_ich_lr0_el2,
    asm::get_ich_lr1_el2,
    asm::get_ich_lr2_el2,
];
const SET_ICH_LRN_EL2: [unsafe fn(u64); 3] = [
    asm::set_ich_lr0_el2,
    asm::set_ich_lr1_el2,
    asm::set_ich_lr2_el2,
];

pub fn init_vgic(redistributor: &GicRedistributor) {
    let ich_hcr_el2 = ICH_HCR_EL2_EN;
    unsafe { asm::set_ich_hcr_el2(ich_hcr_el2) };

    /* Enable Maintenance Interrupt */
    redistributor.set_group(MAINTENANCE_INTERRUPT_INTID, GicGroup::NonSecureGroup1);
    redistributor.set_priority(MAINTENANCE_INTERRUPT_INTID, 0x00);
    redistributor.set_trigger_mode(MAINTENANCE_INTERRUPT_INTID, true);
    redistributor.set_enable(MAINTENANCE_INTERRUPT_INTID, true);

    /* Enable INJECT_INTERRUPT_INT_ID */
    redistributor.set_group(INJECT_INTERRUPT_INT_ID, GicGroup::NonSecureGroup1);
    redistributor.set_priority(INJECT_INTERRUPT_INT_ID, 0x00);
    redistributor.set_trigger_mode(INJECT_INTERRUPT_INT_ID, false);
    redistributor.set_enable(INJECT_INTERRUPT_INT_ID, true);
}

pub fn create_list_register_entry(
    int_id: u32,
    group: u32,
    priority: u32,
    physical_int_id: Option<u32>,
) -> u64 {
    let mut entry = ICH_LRN_EL2_STATUS_PENDING
        | ((group as u64) << 60)
        | ((priority as u64) << 48)
        | (int_id as u64);
    if let Some(p_int_id) = physical_int_id {
        entry |= ICH_LRN_EL2_HW | ((p_int_id as u64) << 32);
    } else {
        entry |= ICH_LRN_EL2_EOI;
    }
    entry
}

pub fn add_virtual_interrupt(entry: u64) {
    let number_of_lrn = (asm::get_ich_vtr_el2() & ICH_VTR_EL2_LIST_REGS) as usize + 1;
    let supported_lrn = number_of_lrn.min(GET_ICH_LRN_EL2.len());

    for i in 0..supported_lrn {
        let ich_lrn_el2 = (GET_ICH_LRN_EL2[i])();
        if (ich_lrn_el2 & ICH_LRN_EL2_STATUS) == ICH_LRN_EL2_STATUS_INACTIVE {
            unsafe { (SET_ICH_LRN_EL2[i])(entry) };
            return;
        } else if ich_lrn_el2 & ((1 << 32) - 1) == entry & ((1 << 32) - 1) {
            unsafe { (SET_ICH_LRN_EL2[i])(ich_lrn_el2 | ICH_LRN_EL2_STATUS_PENDING) };
            return;
        }
    }
    println!("ICH_LRN_EL2 is overflowed.");
}

pub fn maintenance_interrupt_handler() {
    let number_of_lrn = (asm::get_ich_vtr_el2() & ICH_VTR_EL2_LIST_REGS) as usize + 1;
    let supported_lrn = number_of_lrn.min(GET_ICH_LRN_EL2.len());
    let mut eoi_bits = asm::get_ich_eisr_el2();

    for i in 0..supported_lrn {
        if (eoi_bits & 1) != 0 {
            let entry = (GET_ICH_LRN_EL2[i])();
            let int_id = (entry & ICH_LRN_EL2_VINTID) as u32;
            let vm = vm::get_current_vm();
            if int_id >= 32 {
                /* SPI */
                let mut distributor = vm.get_gic_distributor_mmio().lock();
                distributor.change_pending_status(int_id, false);
                distributor.change_active_status(int_id, false);
            } else {
                /* SGI / PPI */
                let mut redistributor = vm.get_gic_redistributor_mmio().lock();
                redistributor.change_pending_status(int_id, false);
                redistributor.change_active_status(int_id, false);
            }
            unsafe { (SET_ICH_LRN_EL2[i])(0) };
        }
        eoi_bits >>= 1;
    }
}
