//!
//! Arm Generic Timer
//!

use crate::asm;
use crate::drivers::gicv3;
use crate::dtb;
use crate::vm;

pub static mut GENERIC_TIMER_PHYSICAL_INT_ID: u32 = 0;
const GENERIC_TIMER_VIRTUAL_INT_ID: u32 = 27;

pub fn init_generic_timer_global(dtb: &dtb::Dtb) {
    let generic_timer_node = dtb
        .search_node_by_compatible(b"arm,armv8-timer", None)
        .expect("Failed to find generic timer");
    let interrupt_number = dtb.read_property_as_u32_array(
        &dtb.get_property(&generic_timer_node, b"interrupts")
            .unwrap(),
    );
    if u32::from_be(interrupt_number[6]) == gicv3::DTB_GIC_PPI {
        unsafe {
            GENERIC_TIMER_PHYSICAL_INT_ID = gicv3::GIC_PPI_BASE + u32::from_be(interrupt_number[7])
        };
    }
}

pub fn init_generic_timer_local(redistributor: &gicv3::GicRedistributor) {
    /* オフセットを0で初期化 */
    unsafe { asm::set_cntvoff_el2(0) };

    /* Generic Timer の割り込みを有効化 */
    let int_id = unsafe { GENERIC_TIMER_PHYSICAL_INT_ID };
    redistributor.set_group(int_id, gicv3::GicGroup::NonSecureGroup1);
    redistributor.set_priority(int_id, 0x00);
    redistributor.set_trigger_mode(int_id, true);
    redistributor.set_enable(int_id, true);
}

pub fn generic_timer_interrupt_handler() {
    let vm = vm::get_current_vm();
    let redistributor = unsafe { &mut *vm.get_gic_redistributor_mmio() };
    redistributor.trigger_interrupt(
        GENERIC_TIMER_VIRTUAL_INT_ID,
        Some(unsafe { GENERIC_TIMER_PHYSICAL_INT_ID }),
    );
}
