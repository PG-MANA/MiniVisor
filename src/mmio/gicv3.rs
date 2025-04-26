//!
//!  Generic Interrupt Controller version 3 MMIO Driver
//!

use crate::asm;
use crate::vgic;
use crate::vm;
use crate::vm::MmioHandler;

use alloc::collections::linked_list::LinkedList;

/* GIC Distributor */
pub struct GicDistributorMmio {
    ctlr: u32,
    group: [u32; 32],
    enable: [u32; 32],
    pending: [u32; 32],
    active: [u32; 32],
    priority: [u32; 255],
    configuration: [u32; 64],
    group_modifier: [u32; 32],
    router: [u64; 1020],
    to_inject_interrupt: LinkedList<u64>,
}

const GIC_REVISION: u64 = 3;

/* Registers */
const GICD_CTLR: usize = 0x0000;
const GICD_TYPER: usize = 0x0004;
const GICD_IGROUPR0: usize = 0x0080;
const GICD_IGROUPR31: usize = 0x00FC;
const GICD_ISENABLER0: usize = 0x0100;
const GICD_ISENABLER31: usize = 0x017C;
const GICD_ICENABLER0: usize = 0x0180;
const GICD_ICENABLER31: usize = 0x01FC;
const GICD_ISPENDR0: usize = 0x0200;
const GICD_ISPENDR31: usize = 0x027C;
const GICD_ICPENDR0: usize = 0x0280;
const GICD_ICPENDR31: usize = 0x02FC;
const GICD_ISACTIVER0: usize = 0x0300;
const GICD_ISACTIVER31: usize = 0x037C;
const GICD_ICACTIVER0: usize = 0x0380;
const GICD_ICACTIVER31: usize = 0x03FC;
const GICD_IPRIORITYR0: usize = 0x0400;
const GICD_IPRIORITYR254: usize = 0x07F8;
const GICD_ICFGR0: usize = 0x0C00;
const GICD_ICFGR63: usize = 0x0CFC;
const GICD_IGRPMODR0: usize = 0x0D00;
const GICD_IGRPMODR31: usize = 0x0F7C;
const GICD_IROUTER0: usize = 0x6100;
const GICD_IROUTER1019: usize = 0x7FD8;
const GICD_PIDR2: usize = 0xFFE8;

const GICD_CTLR_ARE_S: u32 = 1 << 4;
const GICD_CTLR_ENABLE_GRP1NS: u32 = 1 << 1;

const GICD_TYPER_VALUE: u32 = (1 << 25/* No1N */) | (31/* Max SPI INIID(1023)*/);

pub const INJECT_INTERRUPT_INT_ID: u32 = 11;

impl GicDistributorMmio {
    pub const MMIO_SIZE: usize = 0x10000;

    pub fn new() -> Self {
        Self {
            ctlr: 0,
            group: [0; 32],
            enable: [0; 32],
            pending: [0; 32],
            active: [0; 32],
            priority: [0; 255],
            configuration: [0; 64],
            group_modifier: [0; 32],
            router: [0; 1020],
            to_inject_interrupt: LinkedList::new(),
        }
    }

    fn get_group(&self, int_id: u32) -> u32 {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize;
        let register = int_id / DATA_PER_REG;
        let offset = int_id % DATA_PER_REG;
        (self.group[register] >> offset) & 0b1
    }

    fn get_enable(&self, int_id: u32) -> bool {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize;
        let register = int_id / DATA_PER_REG;
        let offset = int_id % DATA_PER_REG;
        ((self.enable[register] >> offset) & 1) != 0
    }

    fn get_pending(&self, int_id: u32) -> bool {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize;
        let register = int_id / DATA_PER_REG;
        let offset = int_id % DATA_PER_REG;
        ((self.pending[register] >> offset) & 1) != 0
    }

    pub fn change_pending_status(&mut self, int_id: u32, is_pending: bool) {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize;
        let register = int_id / DATA_PER_REG;
        let offset = int_id % DATA_PER_REG;
        if is_pending {
            self.pending[register] |= 1 << offset;
        } else {
            self.pending[register] &= !(1 << offset);
        }
    }

    pub fn change_active_status(&mut self, int_id: u32, is_active: bool) {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize;
        let register = int_id / DATA_PER_REG;
        let offset = int_id % DATA_PER_REG;
        if is_active {
            self.active[register] |= 1 << offset;
        } else {
            self.active[register] &= !(1 << offset);
        }
    }

    fn get_priority(&self, int_id: u32) -> u32 {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize / 8;
        let register = int_id / DATA_PER_REG;
        let offset = (int_id % DATA_PER_REG) * 8;
        (self.priority[register] >> offset) & 0xFF
    }

    fn get_group_modifier(&self, int_id: u32) -> u32 {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize;
        let register = int_id / DATA_PER_REG;
        let offset = int_id % DATA_PER_REG;
        (self.group_modifier[register] >> offset) & 0b1
    }

    fn get_router(&self, int_id: u32) -> u64 {
        self.router[int_id as usize]
    }

    pub fn trigger_interrupt(&mut self, int_id: u32, physical_int_id: Option<u32>) {
        if (self.ctlr & GICD_CTLR_ENABLE_GRP1NS) == 0 {
            println!("GIC Distributor is not enabled.");
            return;
        }
        self.change_pending_status(int_id, true);
        if !self.get_enable(int_id) {
            return;
        }
        if self.get_group_modifier(int_id) == 1 {
            println!("Secure Interrupt is not supported.");
            return;
        }
        let group = self.get_group(int_id);
        let priority = self.get_priority(int_id);
        let router = self.get_router(int_id);
        if (self.ctlr & GICD_CTLR_ARE_S) == 0 {
            println!("Target CPU Style interrupt is not supported.");
            return;
        }
        let router_aff3 = (router & ((1 << 40) - 1)) >> 32;
        let router_aff2 = (router & ((1 << 24) - 1)) >> 16;
        let router_aff1 = (router & ((1 << 16) - 1)) >> 8;
        let router_aff0 = router & ((1 << 8) - 1);
        let mpidr_el1 = asm::get_mpidr_el1();
        let mpidr_aff3 = (mpidr_el1 & ((1 << 40) - 1)) >> 32;
        let mpidr_aff2 = (mpidr_el1 & ((1 << 24) - 1)) >> 16;
        let mpidr_aff1 = (mpidr_el1 & ((1 << 16) - 1)) >> 8;
        let mpidr_aff0 = mpidr_el1 & ((1 << 8) - 1);

        let list_entry = vgic::create_list_register_entry(int_id, group, priority, physical_int_id);

        if router_aff3 == mpidr_aff3
            && router_aff2 == mpidr_aff2
            && router_aff1 == mpidr_aff1
            && router_aff0 == mpidr_aff0
        {
            /* 同じpCPU */
            vgic::add_virtual_interrupt(list_entry);
        } else {
            /* 違うVMのpCPU */
            self.to_inject_interrupt.push_back(list_entry);
            assert!(router_aff0 < 16);
            let icc_sgi1r_el1 = (router_aff3 << 48)
                | (router_aff2 << 32)
                | ((INJECT_INTERRUPT_INT_ID as u64) << 24)
                | (router_aff1 << 48)
                | (1 << router_aff0);
            unsafe { asm::set_icc_sgi1r_el1(icc_sgi1r_el1) };
        }
    }
}

pub fn inject_interrupt_handler() {
    let vm = vm::get_current_vm();
    let mut distributor = vm.get_gic_distributor_mmio().lock();
    let mut redistributor = vm.get_gic_redistributor_mmio().lock();
    while let Some(entry) = distributor.to_inject_interrupt.pop_front() {
        vgic::add_virtual_interrupt(entry);
    }
    while let Some(entry) = redistributor.to_inject_interrupt.pop_front() {
        vgic::add_virtual_interrupt(entry);
    }
}

impl MmioHandler for GicDistributorMmio {
    fn read(&mut self, offset: usize, access_width: u64) -> Result<u64, ()> {
        let mut result = 0u64;
        if offset == GICD_CTLR && access_width == 32 {
            result = self.ctlr as u64;
        } else if offset == GICD_TYPER && access_width == 32 {
            result = GICD_TYPER_VALUE as u64;
        } else if (GICD_IGROUPR0..=GICD_IGROUPR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_IGROUPR0) / size_of::<u32>();
            result = self.group[register_offset] as u64;
        } else if (GICD_ISENABLER0..=GICD_ISENABLER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ISENABLER0) / size_of::<u32>();
            result = self.enable[register_offset] as u64;
        } else if (GICD_ICENABLER0..=GICD_ICENABLER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICENABLER0) / size_of::<u32>();
            result = self.enable[register_offset] as u64;
        } else if (GICD_ISPENDR0..=GICD_ISPENDR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ISPENDR0) / size_of::<u32>();
            result = self.pending[register_offset] as u64;
        } else if (GICD_ICPENDR0..=GICD_ICPENDR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICPENDR0) / size_of::<u32>();
            result = self.pending[register_offset] as u64;
        } else if (GICD_ISACTIVER0..=GICD_ISACTIVER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ISACTIVER0) / size_of::<u32>();
            result = self.active[register_offset] as u64;
        } else if (GICD_ICACTIVER0..=GICD_ICACTIVER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICACTIVER0) / size_of::<u32>();
            result = self.active[register_offset] as u64;
        } else if (GICD_IPRIORITYR0..(GICD_IPRIORITYR254 + size_of::<u32>())).contains(&offset) {
            let register_offset = (offset - GICD_IPRIORITYR0) / size_of::<u32>();
            let byte_offset = (offset - GICD_IPRIORITYR0) - register_offset * size_of::<u32>();
            if access_width == 8 {
                /* Byte access */
                result = self.priority[register_offset] as u64;
                result = (result >> (byte_offset * 8)) & 0xff;
            } else if byte_offset == 0 && access_width == 32 {
                /* 32-bit access */
                result = self.priority[register_offset] as u64;
            }
        } else if (GICD_ICFGR0..=GICD_ICFGR63).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICFGR0) / size_of::<u32>();
            result = self.configuration[register_offset] as u64;
        } else if (GICD_IGRPMODR0..=GICD_IGRPMODR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_IGRPMODR0) / size_of::<u32>();
            result = self.group_modifier[register_offset] as u64;
        } else if (GICD_IROUTER0..=GICD_IROUTER1019).contains(&offset) && access_width == 64 {
            let register_offset = (offset - GICD_IROUTER0) / size_of::<u64>();
            result = self.router[register_offset];
        } else if offset == GICD_PIDR2 && access_width == 32 {
            result = GIC_REVISION << 4;
        }
        Ok(result)
    }

    fn write(&mut self, offset: usize, access_width: u64, value: u64) -> Result<(), ()> {
        if offset == GICD_CTLR && access_width == 32 {
            self.ctlr = value as u32;
        } else if (GICD_IGROUPR0..=GICD_IGROUPR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_IGROUPR0) / size_of::<u32>();
            self.group[register_offset] = value as u32;
        } else if (GICD_ISENABLER0..=GICD_ISENABLER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ISENABLER0) / size_of::<u32>();
            self.enable[register_offset] |= value as u32;
            let mut value = value;
            for int_id in (register_offset * 32).. {
                if value == 0 {
                    break;
                }
                if (value & 1) != 0 && self.get_pending(int_id as u32) {
                    self.trigger_interrupt(int_id as u32, None);
                }
                value >>= 1;
            }
        } else if (GICD_ICENABLER0..=GICD_ICENABLER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICENABLER0) / size_of::<u32>();
            self.enable[register_offset] &= !(value as u32);
        } else if (GICD_ISPENDR0..=GICD_ISPENDR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ISPENDR0) / size_of::<u32>();
            self.pending[register_offset] |= value as u32;
            let mut value = value;
            for int_id in (register_offset * size_of::<u32>() * 8).. {
                if value == 0 {
                    break;
                }
                if (value & 1) != 0 && self.get_enable(int_id as u32) {
                    self.trigger_interrupt(int_id as u32, None);
                }
                value >>= 1;
            }
        } else if (GICD_ICPENDR0..=GICD_ICPENDR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICPENDR0) / size_of::<u32>();
            self.pending[register_offset] &= !(value as u32);
        } else if (GICD_ISACTIVER0..=GICD_ISACTIVER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ISACTIVER0) / size_of::<u32>();
            self.active[register_offset] |= value as u32;
        } else if (GICD_ICACTIVER0..=GICD_ICACTIVER31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICACTIVER0) / size_of::<u32>();
            self.active[register_offset] &= !(value as u32);
        } else if (GICD_IPRIORITYR0..(GICD_IPRIORITYR254 + size_of::<u32>())).contains(&offset) {
            let register_offset = (offset - GICD_IPRIORITYR0) / size_of::<u32>();
            let byte_offset = (offset - GICD_IPRIORITYR0) - register_offset * size_of::<u32>();
            let bit_offset = byte_offset * 8;
            if access_width == 8 {
                /* Byte access */
                self.priority[register_offset] = ((self.priority[register_offset])
                    & !(0xFF << bit_offset))
                    | (value << bit_offset) as u32;
            } else if byte_offset == 0 && access_width == 32 {
                /* 32-bit access */
                self.priority[register_offset] = value as u32;
            }
        } else if (GICD_ICFGR0..=GICD_ICFGR63).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_ICFGR0) / size_of::<u32>();
            self.configuration[register_offset] = value as u32;
        } else if (GICD_IGRPMODR0..=GICD_IGRPMODR31).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICD_IGRPMODR0) / size_of::<u32>();
            self.group_modifier[register_offset] = value as u32;
        } else if (GICD_IROUTER0..=GICD_IROUTER1019).contains(&offset) && access_width == 64 {
            let register_offset = (offset - GICD_IROUTER0) / size_of::<u64>();
            self.router[register_offset] = value;
        }
        Ok(())
    }
}

/* GIC Redistributor */
pub struct GicRedistributorMmio {
    affinity: u64,
    ctlr: u32,
    waker: u32,
    group: u32,
    enable: u32,
    pending: u32,
    active: u32,
    priority: [u32; 8],
    configuration: [u32; 2],
    group_modifier: u32,
    to_inject_interrupt: LinkedList<u64>,
}

/* Registers */
const GICR_CTLR: usize = 0x0000;
const GICR_TYPER: usize = 0x0008;
const GICR_WAKER: usize = 0x0014;
const GICR_PIDR2: usize = 0xFFE8;

const GICR_VLPI_BASE: usize = 0x10000;
const GICR_IGROUPR0: usize = GICR_VLPI_BASE + 0x0080;
const GICR_ISENABLER0: usize = GICR_VLPI_BASE + 0x0100;
const GICR_ICENABLER0: usize = GICR_VLPI_BASE + 0x0180;
const GICR_ISPENDR0: usize = GICR_VLPI_BASE + 0x0200;
const GICR_ICPENDR0: usize = GICR_VLPI_BASE + 0x0280;
const GICR_ISACTIVER0: usize = GICR_VLPI_BASE + 0x0300;
const GICR_ICACTIVER0: usize = GICR_VLPI_BASE + 0x0380;
const GICR_IPRIORITYR0: usize = GICR_VLPI_BASE + 0x0400;
const GICR_IPRIORITYR7: usize = GICR_VLPI_BASE + 0x041C;
const GICR_ICFGR0: usize = GICR_VLPI_BASE + 0x0C00;
const GICR_ICFGR1: usize = GICR_VLPI_BASE + 0x0C04;
const GICR_IGRPMODR0: usize = GICR_VLPI_BASE + 0x0D00;

const GICR_TYPER_LAST: u32 = 1 << 4;

const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;

impl GicRedistributorMmio {
    pub const MMIO_SIZE: usize = 0x10000 * 2;

    pub fn new(mpidr_el1: u64) -> Self {
        Self {
            ctlr: 0,
            affinity: asm::mpidr_to_affinity(mpidr_el1),
            waker: GICR_WAKER_CHILDREN_ASLEEP,
            group: 0,
            enable: 0,
            pending: 0,
            active: 0,
            priority: [0; 8],
            configuration: [0; 2],
            group_modifier: 0,
            to_inject_interrupt: LinkedList::new(),
        }
    }

    fn generate_typer(&self) -> u64 {
        let mpidr_aff3 = (self.affinity & ((1 << 40) - 1)) >> 32;
        let mpidr_aff2 = (self.affinity & ((1 << 24) - 1)) >> 16;
        let mpidr_aff1 = (self.affinity & ((1 << 16) - 1)) >> 8;
        let mpidr_aff0 = self.affinity & ((1 << 8) - 1);
        (GICR_TYPER_LAST as u64)
            | (mpidr_aff0 << 32)
            | (mpidr_aff1 << 40)
            | (mpidr_aff2 << 48)
            | (mpidr_aff3 << 56)
    }

    fn get_group(&self, int_id: u32) -> u32 {
        (self.group >> (int_id as usize)) & 0b1
    }

    fn get_enable(&self, int_id: u32) -> bool {
        ((self.enable >> (int_id as usize)) & 1) != 0
    }

    fn get_pending(&self, int_id: u32) -> bool {
        ((self.pending >> (int_id as usize)) & 1) != 0
    }

    pub fn change_pending_status(&mut self, int_id: u32, is_pending: bool) {
        if is_pending {
            self.pending |= 1 << int_id;
        } else {
            self.pending &= !(1 << int_id);
        }
    }

    pub fn change_active_status(&mut self, int_id: u32, is_active: bool) {
        if is_active {
            self.active |= 1 << int_id;
        } else {
            self.active &= !(1 << int_id);
        }
    }

    fn get_priority(&self, int_id: u32) -> u32 {
        let int_id = int_id as usize;
        const DATA_PER_REG: usize = u32::BITS as usize / 8;
        let register = int_id / DATA_PER_REG;
        let offset = (int_id % DATA_PER_REG) * 8;
        (self.priority[register] >> offset) & 0xFF
    }

    fn get_group_modifier(&self, int_id: u32) -> u32 {
        (self.group_modifier >> (int_id as usize)) & 0b1
    }

    pub fn trigger_interrupt(&mut self, int_id: u32, physical_int_id: Option<u32>) {
        if (self.waker & GICR_WAKER_CHILDREN_ASLEEP) != 0 {
            println!("GIC Redistributor is not enabled.");
            return;
        }
        self.change_pending_status(int_id, true);
        if !self.get_enable(int_id) {
            return;
        }
        if self.get_group_modifier(int_id) == 1 {
            println!("Secure Interrupt is not supported.");
            return;
        }
        let group = self.get_group(int_id);
        let priority = self.get_priority(int_id);

        let list_entry = vgic::create_list_register_entry(int_id, group, priority, physical_int_id);
        if self.affinity == asm::mpidr_to_affinity(asm::get_mpidr_el1()) {
            /* 同じpCPU */
            vgic::add_virtual_interrupt(list_entry);
        } else {
            /* 違うVMのpCPU */
            self.to_inject_interrupt.push_back(list_entry);
            let mpidr_aff3 = (self.affinity & ((1 << 40) - 1)) >> 32;
            let mpidr_aff2 = (self.affinity & ((1 << 24) - 1)) >> 16;
            let mpidr_aff1 = (self.affinity & ((1 << 16) - 1)) >> 8;
            let mpidr_aff0 = self.affinity & ((1 << 8) - 1);
            assert!(mpidr_aff0 < 16);
            let icc_sgi1r_el1 = (mpidr_aff3 << 48)
                | (mpidr_aff2 << 32)
                | ((INJECT_INTERRUPT_INT_ID as u64) << 24)
                | (mpidr_aff1 << 48)
                | (1 << mpidr_aff0);
            unsafe { asm::set_icc_sgi1r_el1(icc_sgi1r_el1) };
        }
    }
}

impl MmioHandler for GicRedistributorMmio {
    fn read(&mut self, offset: usize, access_width: u64) -> Result<u64, ()> {
        let mut result = 0u64;
        if offset == GICR_CTLR && access_width == 32 {
            result = self.ctlr as u64;
        } else if offset == GICR_TYPER && (access_width == 32 || access_width == 64) {
            result = self.generate_typer();
        } else if offset == (GICR_TYPER + size_of::<u32>()) && access_width == 32 {
            result = self.generate_typer() >> u32::BITS;
        } else if offset == GICR_WAKER && access_width == 32 {
            result = self.waker as u64;
        } else if offset == GICR_PIDR2 && access_width == 32 {
            result = GIC_REVISION << 4;
        } else if offset == GICR_IGROUPR0 && access_width == 32 {
            result = self.group as u64;
        } else if (offset == GICR_ISENABLER0 || offset == GICR_ICENABLER0) && access_width == 32 {
            result = self.enable as u64;
        } else if (offset == GICR_ISPENDR0 || offset == GICR_ICPENDR0) && access_width == 32 {
            result = self.pending as u64;
        } else if (offset == GICR_ISACTIVER0 || offset == GICR_ICACTIVER0) && access_width == 32 {
            result = self.active as u64;
        } else if (GICR_IPRIORITYR0..=GICR_IPRIORITYR7).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICR_IPRIORITYR0) / size_of::<u32>();
            result = self.priority[register_offset] as u64;
        } else if (GICR_ICFGR0..=GICR_ICFGR1).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICR_ICFGR0) / size_of::<u32>();
            result = self.configuration[register_offset] as u64;
        } else if offset == GICR_IGRPMODR0 && access_width == 32 {
            result = self.group_modifier as u64;
        }
        Ok(result)
    }

    fn write(&mut self, offset: usize, access_width: u64, value: u64) -> Result<(), ()> {
        if offset == GICR_CTLR && access_width == 32 {
            self.ctlr = value as u32;
        }
        if offset == GICR_WAKER && access_width == 32 {
            if ((value as u32) & GICR_WAKER_PROCESSOR_SLEEP) == 0 {
                self.waker = 0;
            } else {
                self.waker = GICR_WAKER_CHILDREN_ASLEEP | GICR_WAKER_PROCESSOR_SLEEP;
            }
        } else if offset == GICR_IGROUPR0 && access_width == 32 {
            self.group = value as u32;
        } else if offset == GICR_ISENABLER0 && access_width == 32 {
            self.enable |= value as u32;
            let mut value = value;
            for int_id in 0..32 {
                if value == 0 {
                    break;
                }
                if (value & 1) != 0 && self.get_pending(int_id) {
                    self.trigger_interrupt(int_id, None);
                }
                value >>= 1;
            }
        } else if offset == GICR_ICENABLER0 && access_width == 32 {
            self.enable &= !(value as u32);
        } else if offset == GICR_ISPENDR0 && access_width == 32 {
            self.pending |= value as u32;
            let mut value = value;
            for int_id in 0..32 {
                if value == 0 {
                    break;
                }
                if (value & 1) != 0 && self.get_enable(int_id) {
                    self.trigger_interrupt(int_id, None);
                }
                value >>= 1;
            }
        } else if offset == GICR_ICPENDR0 && access_width == 32 {
            self.pending &= !(value as u32);
        } else if offset == GICR_ISACTIVER0 && access_width == 32 {
            self.active |= value as u32;
        } else if offset == GICR_ICACTIVER0 && access_width == 32 {
            self.active &= !(value as u32);
        } else if (GICR_IPRIORITYR0..(GICR_IPRIORITYR7 + size_of::<u32>())).contains(&offset) {
            let register_offset = (offset - GICR_IPRIORITYR0) / size_of::<u32>();
            let byte_offset = (offset - GICR_IPRIORITYR0) - register_offset * size_of::<u32>();
            let bit_offset = byte_offset * 8;
            if access_width == 8 {
                /* Byte access */
                self.priority[register_offset] = ((self.priority[register_offset])
                    & !(0xFF << bit_offset))
                    | (value << bit_offset) as u32;
            } else if byte_offset == 0 && access_width == 32 {
                /* 32-bit access */
                self.priority[register_offset] = value as u32;
            }
        } else if (GICR_ICFGR0..=GICR_ICFGR1).contains(&offset) && access_width == 32 {
            let register_offset = (offset - GICR_ICFGR0) / size_of::<u32>();
            self.configuration[register_offset] = value as u32;
        } else if offset == GICR_IGRPMODR0 && access_width == 32 {
            self.group_modifier = value as u32;
        }
        Ok(())
    }
}
