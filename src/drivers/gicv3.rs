//!
//! GICv3 割り込み管理モジュール
//!

use crate::asm;

pub const DTB_GIC_LEVEL: u32 = 4;
pub const DTB_GIC_SPI: u32 = 0;
pub const GIC_SPI_BASE: u32 = 32;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum GicGroup {
    NonSecureGroup1,
}

pub struct GicDistributor {
    base_address: usize,
}

pub struct GicRedistributor {
    base_address: usize,
}

impl GicDistributor {
    const GICD_MMIO_SIZE: usize = 0x10000;
    const GICD_CTLR: usize = 0x00;
    const GICD_CTLR_RWP: u32 = 1 << 31;
    const GICD_CTLR_ARE: u32 = 1 << 5;
    const GICD_CTLR_ENABLE_GRP1NS: u32 = 1 << 1;
    const GICD_IGROUPR: usize = 0x0080;
    const GICD_ISENABLER: usize = 0x0100;
    const GICD_ICENABLER: usize = 0x0180;
    const GICD_ISPENDR: usize = 0x0200;
    const GICD_ICPENDR: usize = 0x0280;
    const GICD_IPRIORITYR: usize = 0x0400;
    const GICD_ICFGR: usize = 0x0C00;
    const GICD_IGRPMODR: usize = 0x0D00;
    const GICD_IROUTER: usize = 0x6100;

    pub fn new(base_address: usize, size: usize) -> Result<Self, ()> {
        if size != Self::GICD_MMIO_SIZE {
            println!("Invalid GICD Size: {:#X}", size);
            return Err(());
        }
        Ok(Self { base_address })
    }

    pub fn init(&self) {
        self.write_register(Self::GICD_CTLR, Self::GICD_CTLR_ARE);
        self.wait_rwp();
        self.write_register(
            Self::GICD_CTLR,
            Self::GICD_CTLR_ARE | Self::GICD_CTLR_ENABLE_GRP1NS,
        );
    }

    pub fn set_priority(&self, int_id: u32, priority: u8) {
        let register_index = ((int_id >> 2) as usize) * size_of::<u32>();
        let register_offset = (int_id & 0b11) << 3;
        self.write_register(
            Self::GICD_IPRIORITYR + register_index,
            (self.read_register(Self::GICD_IPRIORITYR + register_index)
                & !(0xFF << register_offset))
                | ((priority as u32) << register_offset),
        );
    }

    pub fn set_group(&self, int_id: u32, group: GicGroup) {
        let register_index = ((int_id / u32::BITS) as usize) * size_of::<u32>();
        let register_offset = int_id & (u32::BITS - 1);
        let data = match group {
            GicGroup::NonSecureGroup1 => 1,
        };
        self.write_register(
            Self::GICD_IGROUPR + register_index,
            (self.read_register(Self::GICD_IGROUPR + register_index) & !(1 << register_offset))
                | (data << register_offset),
        );
        let data = match group {
            GicGroup::NonSecureGroup1 => 0,
        };
        self.write_register(
            Self::GICD_IGRPMODR + register_index,
            (self.read_register(Self::GICD_IGRPMODR + register_index) & !(1 << register_offset))
                | (data << register_offset),
        );
    }

    pub fn set_enable(&self, int_id: u32, enable: bool) {
        let register_index = ((int_id / u32::BITS) as usize) * size_of::<u32>();
        let register_offset = int_id & (u32::BITS - 1);
        let register = if enable {
            Self::GICD_ISENABLER
        } else {
            Self::GICD_ICENABLER
        };

        self.write_register(register + register_index, 1 << register_offset);
    }

    pub fn set_pending(&self, int_id: u32, pending: bool) {
        let register_index = ((int_id / u32::BITS) as usize) * size_of::<u32>();
        let register_offset = int_id & (u32::BITS - 1);
        let register = if pending {
            Self::GICD_ISPENDR
        } else {
            Self::GICD_ICPENDR
        };
        self.write_register(
            register + register_index,
            self.read_register(register + register_index) | (1 << register_offset),
        );
    }

    pub fn set_trigger_mode(&self, int_id: u32, is_level_trigger: bool) {
        let register_index = ((int_id / (u32::BITS / 2)) as usize) * size_of::<u32>();
        let register_offset = (int_id & (u32::BITS / 2 - 1)) * 2;

        self.write_register(
            Self::GICD_ICFGR + register_index,
            (self.read_register(Self::GICD_ICFGR + register_index) & !(0x03 << register_offset))
                | ((((!is_level_trigger) as u32) << 1) << register_offset),
        );
    }

    pub fn set_routing(&self, int_id: u32, is_routing_mode: bool, mpidr: u64) {
        if is_routing_mode {
            unimplemented!()
        } else {
            unsafe {
                core::ptr::write_volatile(
                    (self.base_address + Self::GICD_IROUTER + (int_id as usize) * size_of::<u64>())
                        as *mut u64,
                    asm::mpidr_to_affinity(mpidr),
                )
            }
        }
    }

    fn wait_rwp(&self) {
        while (self.read_register(Self::GICD_CTLR) & Self::GICD_CTLR_RWP) != 0 {
            core::hint::spin_loop();
        }
    }

    fn read_register(&self, register: usize) -> u32 {
        unsafe { core::ptr::read_volatile((self.base_address + register) as *const u32) }
    }

    fn write_register(&self, register: usize, data: u32) {
        unsafe { core::ptr::write_volatile((self.base_address + register) as *mut u32, data) }
    }
}

impl GicRedistributor {
    pub const GICR_MMIO_SIZE: usize = 0x20000;
    pub const DEFAULT_PRIORITY: u8 = 0xff;
    pub const DEFAULT_BINARY_POINT: u8 = 0x03;

    const ICC_SRE_SRE: u64 = 1;
    const ICC_IGRPEN1_EN: u64 = 1;

    const GICR_CTLR: usize = 0x00;
    const GICR_CTLR_RWP: u32 = 1 << 3;

    const GICR_TYPER: usize = 0x08;

    const GICR_WAKER: usize = 0x0014;
    const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;
    const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;

    const GICR_IGROUPR0: usize = 0x10000 + 0x0080;
    const GICR_IPRIORITYR_BASE: usize = 0x10000 + 0x0400;
    const GICR_IGRPMODR0: usize = 0x10000 + 0x0D00;
    const GICR_ISENABLER0: usize = 0x10000 + 0x0100;
    const GICR_ICENABLER0: usize = 0x10000 + 0x0180;
    const GICR_ICFGR0: usize = 0x10000 + 0x0C00;

    fn new(base_address: usize) -> Self {
        Self { base_address }
    }

    pub fn get_affinity(&self) -> u32 {
        (unsafe { core::ptr::read_volatile((self.base_address + Self::GICR_TYPER) as *const u64) }
            >> 32) as u32
    }

    pub fn init(&self) {
        unsafe { asm::set_icc_sre_el2(asm::get_icc_sre_el2() | Self::ICC_SRE_SRE) };
        if (asm::get_icc_sre_el2() & Self::ICC_SRE_SRE) == 0 {
            panic!("GICv3 System Registers is disabled.");
        }
        self.wait_rwp();
        self.write_register(
            Self::GICR_WAKER,
            self.read_register(Self::GICR_WAKER) & !Self::GICR_WAKER_PROCESSOR_SLEEP,
        );
        while (self.read_register(Self::GICR_WAKER) & Self::GICR_WAKER_CHILDREN_ASLEEP) != 0 {
            core::hint::spin_loop();
        }
        self.set_priority_mask(Self::DEFAULT_PRIORITY);
        self.set_binary_point(Self::DEFAULT_BINARY_POINT);
        unsafe { asm::set_icc_igrpen1_el1(Self::ICC_IGRPEN1_EN) };
    }

    pub fn set_priority_mask(&self, mask: u8) {
        unsafe { asm::set_icc_pmr_el1(mask as u64) }
    }

    pub fn set_binary_point(&self, point: u8) {
        unsafe {
            asm::set_icc_bpr1_el1(point as u64);
        }
    }

    pub fn set_priority(&self, int_id: u32, priority: u8) {
        let register_index = ((int_id >> 2) as usize) * size_of::<u32>();
        let register_offset = (int_id & 0b11) << 3;
        self.write_register(
            Self::GICR_IPRIORITYR_BASE + register_index,
            (self.read_register(Self::GICR_IPRIORITYR_BASE + register_index)
                & !(0xFF << register_offset))
                | ((priority as u32) << register_offset),
        );
    }

    pub fn set_group(&self, int_id: u32, group: GicGroup) {
        let data = match group {
            GicGroup::NonSecureGroup1 => 1,
        };
        self.write_register(
            Self::GICR_IGROUPR0,
            (self.read_register(Self::GICR_IGROUPR0) & !(1 << int_id)) | ((data) << int_id),
        );

        let data = match group {
            GicGroup::NonSecureGroup1 => 0,
        };
        self.write_register(
            Self::GICR_IGRPMODR0,
            (self.read_register(Self::GICR_IGRPMODR0) & !(1 << int_id)) | ((data) << int_id),
        );
    }

    pub fn set_enable(&self, int_id: u32, enable: bool) {
        let register = if enable {
            Self::GICR_ISENABLER0
        } else {
            Self::GICR_ICENABLER0
        };

        self.write_register(register, 1 << int_id);
    }

    pub fn set_trigger_mode(&self, int_id: u32, is_level_trigger: bool) {
        let register_index = ((int_id / (u32::BITS / 2)) as usize) * size_of::<u32>();
        let register_offset = (int_id & (u32::BITS / 2 - 1)) * 2;

        self.write_register(
            Self::GICR_ICFGR0 + register_index,
            (self.read_register(Self::GICR_ICFGR0 + register_index) & !(0x03 << register_offset))
                | ((((!is_level_trigger) as u32) << 1) << register_offset),
        );
    }

    pub fn get_acknowledge() -> (u32, GicGroup) {
        (asm::get_icc_iar1_el1() as u32, GicGroup::NonSecureGroup1)
    }

    pub fn send_eoi(int_id: u32, group: GicGroup) {
        match group {
            GicGroup::NonSecureGroup1 => {
                unsafe { asm::set_icc_eoir1_el1(int_id as u64) };
            }
        }
    }

    fn wait_rwp(&self) {
        while (self.read_register(Self::GICR_CTLR) & Self::GICR_CTLR_RWP) != 0 {
            core::hint::spin_loop();
        }
    }

    fn read_register(&self, register: usize) -> u32 {
        unsafe { core::ptr::read_volatile((self.base_address + register) as *const u32) }
    }

    fn write_register(&self, register: usize, data: u32) {
        unsafe { core::ptr::write_volatile((self.base_address + register) as *mut u32, data) }
    }
}

pub fn get_self_redistributor(
    mut base_address: usize,
    length: usize,
) -> Result<GicRedistributor, ()> {
    let limit = base_address + length;
    let affinity = asm::get_packed_affinity();
    while base_address < limit {
        let r = GicRedistributor::new(base_address);
        if r.get_affinity() == affinity {
            return Ok(r);
        }
        base_address += GicRedistributor::GICR_MMIO_SIZE;
    }
    Err(())
}
