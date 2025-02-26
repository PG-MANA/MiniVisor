//!
//! レジスタの定数値を列挙するためのモジュール
//!

/* HCR_EL2 */
pub const HCR_EL2_API: u64 = 1 << 41;
pub const HCR_EL2_RW: u64 = 1 << 31;
pub const HCR_EL2_VM: u64 = 1 << 0;

/* SPSR_EL2 */
pub const SPSR_EL2_M_EL1H: u64 = 0b0101;

/* VTTBR_EL2 */
pub const VTTBR_BADDR: u64 = ((1 << 47) - 1) & !1;

/* VTCR_EL2 */
pub const VTCR_EL2_RES1: u64 = 1 << 31;
pub const VTCR_EL2_PS_BITS_OFFSET: u64 = 16;
pub const VTCR_EL2_TG0_BITS_OFFSET: u64 = 14;
pub const VTCR_EL2_SH0_BITS_OFFSET: u64 = 12;
pub const VTCR_EL2_ORGN0_BITS_OFFSET: u64 = 10;
pub const VTCR_EL2_IRGN0_BITS_OFFSET: u64 = 8;
pub const VTCR_EL2_SL0_BITS_OFFSET: u64 = 6;
pub const VTCR_EL2_SL0: u64 = 0b11 << VTCR_EL2_SL0_BITS_OFFSET;
pub const VTCR_EL2_T0SZ_BITS_OFFSET: u64 = 0;
pub const VTCR_EL2_T0SZ: u64 = 0b111111 << VTCR_EL2_T0SZ_BITS_OFFSET;

/* ID_AA64MMFR0_EL1 */
pub const ID_AA64MMFR0_EL1_PARANGE: u64 = 0b1111;
