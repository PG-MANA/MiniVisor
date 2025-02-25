//!
//! レジスタの定数値を列挙するためのモジュール
//!

/* HCR_EL2 */
pub const HCR_EL2_API: u64 = 1 << 41;
pub const HCR_EL2_RW: u64 = 1 << 31;

/* SPSR_EL2 */
pub const SPSR_EL2_M_EL1H: u64 = 0b0101;
