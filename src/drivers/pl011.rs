//!
//! Arm PL011のデバイスドライバ
//!
use crate::serial;

use core::fmt::Error;
use core::ptr;

pub struct Pl011 {
    base_address: usize,
    pub interrupt_number: u32,
}

const UART_SIZE: usize = 0x1000;

const UART_DR: usize = 0x000;
const UART_FR: usize = 0x018;
const UART_CR: usize = 0x030;
const UART_IMSC: usize = 0x038;

/// TX FIFO が一杯か示すビット
const UART_FR_TXFF: u16 = 1 << 5;
/// RX FIFO が空か示すビット
const UART_FR_RXFE: u16 = 1 << 4;
/// 受信が有効か示すビット
const UART_CR_RXE: u16 = 1 << 9;
/// 送信が有効か表すビット
const UART_CR_TXE: u16 = 1 << 8;
/// UARTが有効か示すビット
const UART_CR_UARTEN: u16 = 1;
/// 受信割り込みが有効か示すビット
const UART_IMSC_RXIM: u16 = 1 << 4;

impl Pl011 {
    pub fn new(base_address: usize, range: usize, interrupt_number: u32) -> Result<Self, ()> {
        if range < UART_SIZE {
            return Err(());
        }
        Ok(Self {
            base_address,
            interrupt_number,
        })
    }

    fn is_tx_fifo_full(&self) -> bool {
        (unsafe { ptr::read_volatile((self.base_address + UART_FR) as *const u16) } & UART_FR_TXFF)
            != 0
    }

    fn is_rx_fifo_empty(&self) -> bool {
        (unsafe { ptr::read_volatile((self.base_address + UART_FR) as *const u16) } & UART_FR_RXFE)
            != 0
    }

    pub fn enable_interrupt(&self) {
        unsafe {
            ptr::write_volatile(
                (self.base_address + UART_CR) as *mut u16,
                UART_CR_RXE | UART_CR_TXE | UART_CR_UARTEN,
            );
            ptr::write_volatile(
                (self.base_address + UART_IMSC) as *mut u16,
                ptr::read_volatile((self.base_address + UART_IMSC) as *const u16) | UART_IMSC_RXIM,
            );
        }
    }
}

/// Serial構造体で使うために必要な実装
impl serial::SerialDevice for Pl011 {
    fn putc(&self, c: u8) -> Result<(), Error> {
        while self.is_tx_fifo_full() {
            core::hint::spin_loop();
        }
        unsafe { ptr::write_volatile((self.base_address + UART_DR) as *mut u8, c) };
        Ok(())
    }

    fn getc(&self) -> Result<Option<u8>, Error> {
        if self.is_rx_fifo_empty() {
            return Ok(None);
        }
        Ok(Some(unsafe {
            ptr::read_volatile((self.base_address + UART_DR) as *const u8)
        }))
    }
}
