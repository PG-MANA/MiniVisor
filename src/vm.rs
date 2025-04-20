//!
//! Virtual Machine の管理モジュール
//!

use crate::asm;
use crate::drivers::{generic_timer, gicv3::GicRedistributor, virtio_blk::VirtioBlk};
use crate::fat32::Fat32;
use crate::mmio::{
    gicv3::{GicDistributorMmio, GicRedistributorMmio},
    pl011::Pl011Mmio,
};
use crate::paging::*;
use crate::registers::*;
use crate::serial::SerialDevice;
use crate::vgic;

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
    gic_distributor_mmio: *mut GicDistributorMmio,
    gic_redistributor_mmio: *mut GicRedistributorMmio,
    pl011_mmio: *mut Pl011Mmio,
}

#[repr(C)]
struct KernelHeader {
    code0: u32,
    code1: u32,
    text_offset: u64,
    image_size: u64,
    flags: u64,
    res2: u64,
    res3: u64,
    res4: u64,
    magic: u32,
    res5: u32,
}

static mut VM_LIST: LinkedList<VM> = LinkedList::new();
static mut NEXT_VM_ID: usize = 0;

impl VM {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        vm_id: usize,
        ram_virtual_base_address: usize,
        ram_physical_base_address: usize,
        ram_size: usize,
        mmio_handlers: LinkedList<MmioEntry>,
        gic_distributor_mmio: *mut GicDistributorMmio,
        gic_redistributor_mmio: *mut GicRedistributorMmio,
        pl011_mmio: *mut Pl011Mmio,
    ) -> Self {
        Self {
            vm_id,
            ram_virtual_base_address,
            ram_physical_base_address,
            ram_size,
            mmio_handlers,
            gic_distributor_mmio,
            gic_redistributor_mmio,
            pl011_mmio,
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

    pub fn get_physical_address(&self, virtual_address: usize) -> Option<usize> {
        if (self.ram_virtual_base_address..(self.ram_virtual_base_address + self.ram_size))
            .contains(&virtual_address)
        {
            Some(virtual_address - self.ram_virtual_base_address + self.ram_physical_base_address)
        } else {
            None
        }
    }

    pub fn get_gic_distributor_mmio(&self) -> *mut GicDistributorMmio {
        self.gic_distributor_mmio
    }

    pub fn get_gic_redistributor_mmio(&self) -> *mut GicRedistributorMmio {
        self.gic_redistributor_mmio
    }

    pub fn get_pl011_mmio(&self) -> *mut Pl011Mmio {
        self.pl011_mmio
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

pub fn create_vm(
    fat32: &Fat32,
    blk: &mut VirtioBlk,
    gic_redistributor: &GicRedistributor,
) -> (usize, usize) {
    const RAM_VIRTUAL_BASE: usize = 0x40000000;
    /// RAM SIZE: 256MiB
    const RAM_SIZE: usize = 0x10000000;
    const ALIGN_SIZE: usize = 0x200000;

    /* 仮想マシンの基本要素の設定 */
    let ram_physical_address = crate::allocate_pages(RAM_SIZE >> PAGE_SHIFT, PAGE_SHIFT)
        .expect("Failed to allocate memory for VM.");
    let vm_id = unsafe { NEXT_VM_ID };
    unsafe { NEXT_VM_ID += 1 };
    let cpu_mpidr = asm::get_mpidr_el1();

    /* 仮想化に関するハードウェアの設定 */
    /* レジスタのセットアップ */
    setup_hypervisor_registers();

    /* Stage 2 Translation の初期化 */
    init_stage2_translation_table();
    map_address_stage2(ram_physical_address, RAM_VIRTUAL_BASE, RAM_SIZE, true, true)
        .expect("Failed to map memory");

    /* Virtual GICの初期化 */
    vgic::init_vgic(gic_redistributor);

    /* Generic Timerの初期化 */
    generic_timer::init_generic_timer_local(gic_redistributor);

    /* MMIO ハンドラの初期化 */
    let mut mmio_handlers = LinkedList::new();

    /* PL011 */
    let mut pl011_mmio = Box::new(Pl011Mmio::new());
    let pl011_mmio_ptr = pl011_mmio.as_mut() as *mut _;
    mmio_handlers.push_back(MmioEntry::new(0x9000000, 0x1000, pl011_mmio));

    /* GIC Distributor */
    let mut gic_distributor_mmio = Box::new(GicDistributorMmio::new());
    let gic_distributor_mmio_ptr = gic_distributor_mmio.as_mut() as *mut _;
    mmio_handlers.push_back(MmioEntry::new(
        0x8000000,
        GicDistributorMmio::MMIO_SIZE,
        gic_distributor_mmio,
    ));

    /* GIC Redistributor */
    let mut gic_redistributor_mmio = Box::new(GicRedistributorMmio::new(cpu_mpidr));
    let gic_redistributor_mmio_ptr = gic_redistributor_mmio.as_mut() as *mut _;
    mmio_handlers.push_back(MmioEntry::new(
        0x80a0000,
        GicRedistributorMmio::MMIO_SIZE,
        gic_redistributor_mmio,
    ));

    /* VM構造体の作成 */
    let vm = VM::new(
        vm_id,
        RAM_VIRTUAL_BASE,
        ram_physical_address,
        RAM_SIZE,
        mmio_handlers,
        gic_distributor_mmio_ptr,
        gic_redistributor_mmio_ptr,
        pl011_mmio_ptr,
    );

    /* Linux KernelとDevicetreeの読み込み */
    let kernel = fat32.search_file("IMAGE").unwrap();
    let dtb = fat32.search_file("DTB").unwrap();
    let dtb_size = dtb.get_file_size();
    let kernel_size = kernel.get_file_size();
    let kernel_virtual_address =
        ((RAM_VIRTUAL_BASE + dtb_size - 1) & !(ALIGN_SIZE - 1)) + ALIGN_SIZE;
    let kernel_physical_address = vm.get_physical_address(kernel_virtual_address).unwrap();

    fat32
        .read(&dtb, blk, ram_physical_address, 0, dtb_size)
        .expect("Failed to read DTB");
    fat32
        .read(&kernel, blk, kernel_physical_address, 0, kernel_size)
        .expect("Failed to read Kernel");

    /* Linux Kernel Headerの解析 */
    let header = unsafe { &*(kernel_physical_address as *const KernelHeader) };
    if header.magic != 0x644D5241 {
        panic!("Invalid Kernel Magic: {:#X}", header.magic);
    }
    let mut text_offset = header.text_offset;
    let image_size = header.image_size;
    if image_size == 0 {
        text_offset = 0x80000;
    }

    /* VM構造体のリストへの追加 */
    unsafe { (&raw mut VM_LIST).as_mut().unwrap().push_back(vm) };

    (
        kernel_virtual_address + text_offset as usize,
        RAM_VIRTUAL_BASE,
    )
}

pub fn boot_vm(entry_point: usize, argument: usize) -> ! {
    unsafe {
        /* 仮想マシンの起動 */
        asm::set_spsr_el2(SPSR_EL2_M_EL1H);
        asm::set_elr_el2(entry_point as u64);
        asm::eret(argument as u64, 0, 0, 0);
    }
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

pub fn input_uart(device: &dyn SerialDevice) {
    let c = device.getc();
    if c.is_err() {
        println!("Failed to get a character");
        return;
    }
    let c = c.unwrap().unwrap_or(0);
    if c == 0 {
        return;
    }

    let vm = get_active_vm();
    unsafe { (*vm.get_pl011_mmio()).push(c, &mut *vm.get_gic_distributor_mmio()) };
}

/// 今は一つだけ
pub fn get_current_vm() -> &'static mut VM {
    unsafe { (&raw mut VM_LIST).as_mut().unwrap().front_mut().unwrap() }
}

/// 今は一つだけ
pub fn get_active_vm() -> &'static mut VM {
    unsafe { (&raw mut VM_LIST).as_mut().unwrap().front_mut().unwrap() }
}
