//!
//! PL011 の MMIO Driver
//!

use crate::vm::MmioHandler;

const UART_DR: usize = 0x000;
const UART_FR: usize = 0x018;
const UART_CR: usize = 0x030;
const UART_IMSC: usize = 0x038;

/// RX FIFO が空か示すビット
const UART_FR_RXFE: u16 = 1 << 4;

pub struct Pl011Mmio {
    flag: u16,
    interrupt_mask: u16,
    control: u16,
    read_buffer: [u8; 4],
}

impl Pl011Mmio {
    pub fn new() -> Self {
        Self {
            flag: 0,
            interrupt_mask: 0,
            control: 0,
            read_buffer: [0; 4],
        }
    }
}

impl MmioHandler for Pl011Mmio {
    fn read(&mut self, offset: usize, _access_width: u64) -> Result<u64, ()> {
        let value: u64;
        match offset {
            UART_DR => {
                value = self.read_buffer[0] as u64;
                for i in 1..(self.read_buffer.len()) {
                    self.read_buffer[i - 1] = self.read_buffer[i];
                }
                if self.read_buffer[0] == 0 {
                    self.flag |= UART_FR_RXFE;
                }
            }
            UART_FR => {
                value = self.flag as u64;
            }
            UART_CR => {
                value = self.control as u64;
            }
            UART_IMSC => {
                value = self.interrupt_mask as u64;
            }
            _ => {
                return Err(()); /* unimplemented */
            }
        }
        Ok(value)
    }

    fn write(&mut self, offset: usize, _access_width: u64, value: u64) -> Result<(), ()> {
        match offset {
            UART_DR => {
                print!("{}", value as u8 as char);
            }
            UART_CR => {
                self.control = value as u16;
            }
            UART_IMSC => {
                self.interrupt_mask = value as u16;
            }
            _ => {
                return Err(()); /* unimplemented */
            }
        }
        Ok(())
    }
}
