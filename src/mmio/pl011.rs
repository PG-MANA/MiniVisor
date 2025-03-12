//!
//! PL011 の MMIO Driver
//!

const UART_DR: usize = 0x000;
const UART_FR: usize = 0x018;

pub fn mmio_read(offset: usize, _access_width: u64) -> Result<u64, ()> {
    match offset {
        UART_FR => Ok(0),
        _ => {
            Err(()) /* unimplemented */
        }
    }
}

pub fn mmio_write(offset: usize, _access_width: u64, value: u64) -> Result<(), ()> {
    match offset {
        UART_DR => {
            print!("{}", value as u8 as char);
            Ok(())
        }
        _ => {
            Err(()) /* unimplemented */
        }
    }
}
