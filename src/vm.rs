//!
//! Virtual Machine の管理モジュール
//!

use crate::asm;
use crate::mmio::pl011::Pl011Mmio;
use crate::paging::*;
use crate::registers::*;

use alloc::boxed::Box;
use alloc::collections::linked_list::LinkedList;

pub trait MmioHandler {
    fn read(&mut self, offset: usize, access_width: u64) -> Result<u64, ()>;
    fn write(&mut self, offset: usize, access_width: u64, value: u64) -> Result<(), ()>;
}

pub struct MmioEntry {
    base_address: usize,
    length: usize,
    handler: Box<dyn MmioHandler>,
}

pub struct VM {
    vm_id: usize,
    ram_virtual_base_address: usize,
    ram_physical_base_address: usize,
    ram_size: usize,
    mmio_handlers: LinkedList<MmioEntry>,
}

static mut VM_LIST: LinkedList<VM> = LinkedList::new();
static mut NEXT_VM_ID: usize = 0;

impl VM {
    pub const fn new(
        vm_id: usize,
        ram_virtual_base_address: usize,
        ram_physical_base_address: usize,
        ram_size: usize,
        mmio_handlers: LinkedList<MmioEntry>,
    ) -> Self {
        Self {
            vm_id,
            ram_virtual_base_address,
            ram_physical_base_address,
            ram_size,
            mmio_handlers,
        }
    }

    pub fn handle_mmio_read(&mut self, address: usize, access_width: u64) -> Result<u64, ()> {
        for e in &mut self.mmio_handlers {
            if e.base_address <= address && address < (e.base_address + e.length) {
                return e.handler.read(address - e.base_address, access_width);
            }
        }
        Err(())
    }

    pub fn handle_mmio_write(
        &mut self,
        address: usize,
        access_width: u64,
        value: u64,
    ) -> Result<(), ()> {
        for e in &mut self.mmio_handlers {
            if e.base_address <= address && address < (e.base_address + e.length) {
                return e
                    .handler
                    .write(address - e.base_address, access_width, value);
            }
        }
        Err(())
    }
}

impl MmioEntry {
    pub fn new(base_address: usize, length: usize, handler: Box<dyn MmioHandler>) -> Self {
        Self {
            base_address,
            length,
            handler,
        }
    }
}

pub fn create_vm() {
    const RAM_VIRTUAL_BASE: usize = 0x40000000;
    /// RAM SIZE: 256MiB
    const RAM_SIZE: usize = 0x10000000;

    /* 仮想マシンの基本要素の設定 */
    let ram_physical_address = crate::allocate_pages(RAM_SIZE >> PAGE_SHIFT, PAGE_SHIFT)
        .expect("Failed to allocate memory for VM.");
    let vm_id = unsafe { NEXT_VM_ID };
    unsafe { NEXT_VM_ID += 1 };

    /* 仮想化に関するハードウェアの設定 */
    /* レジスタのセットアップ */
    setup_hypervisor_registers();

    /* Stage 2 Translation の初期化 */
    init_stage2_translation_table();
    map_address_stage2(ram_physical_address, RAM_VIRTUAL_BASE, RAM_SIZE, true, true)
        .expect("Failed to map memory");

    /* MMIO ハンドラの初期化 */
    let mut mmio_handlers = LinkedList::new();

    /* PL011 */
    mmio_handlers.push_back(MmioEntry::new(
        0x9000000,
        0x1000,
        Box::new(Pl011Mmio::new()),
    ));

    /* VM構造体の作成 */
    let vm = VM::new(
        vm_id,
        RAM_VIRTUAL_BASE,
        ram_physical_address,
        RAM_SIZE,
        mmio_handlers,
    );

    /* VM構造体のリストへの追加 */
    unsafe { (&raw mut VM_LIST).as_mut().unwrap().push_back(vm) };
}

fn setup_hypervisor_registers() {
    /* MIDR_EL1 */
    unsafe { asm::set_vpidr_el2(asm::get_midr_el1()) };

    /* MPIDR_EL1 */
    unsafe { asm::set_vmpidr_el2(asm::get_mpidr_el1()) };

    /* HCR_EL2 */
    let hcr_el2 = HCR_EL2_RW | HCR_EL2_API | HCR_EL2_AMO | HCR_EL2_IMO | HCR_EL2_FMO | HCR_EL2_VM;
    unsafe { asm::set_hcr_el2(hcr_el2) };
}

/// 今は一つだけ
pub fn get_current_vm() -> &'static mut VM {
    unsafe { (&raw mut VM_LIST).as_mut().unwrap().front_mut().unwrap() }
}
