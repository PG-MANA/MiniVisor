//!
//! Console
//!

use core::str::SplitWhitespace;

pub struct Console {
    buffer: [u8; Self::BUFFER_SIZE],
    buffer_pointer: usize,
    ignore_lf: bool,
}

impl Console {
    const BUFFER_SIZE: usize = 64;
    #[allow(clippy::type_complexity)]
    const COMMAND_LIST: [(&str, fn(SplitWhitespace)); 2] =
        [("echo", Self::echo), ("poweroff", Self::power_off)];

    pub const fn new() -> Self {
        Self {
            buffer: [0; Self::BUFFER_SIZE],
            buffer_pointer: 0,
            ignore_lf: false,
        }
    }

    pub fn write(&mut self, data: u8) {
        if data == b'\r' || (data == b'\n' && !self.ignore_lf) {
            self.ignore_lf = data == b'\r';
            println!();
            self.exec_command();
            return;
        }
        if data.is_ascii_control() || self.buffer_pointer == Self::BUFFER_SIZE {
            return;
        }
        self.buffer[self.buffer_pointer] = data;
        self.buffer_pointer += 1;
        print!("{}", data as char);
    }

    pub fn exec_command(&mut self) {
        if self.buffer_pointer == 0 {
            self.reset_buffer();
            return;
        }
        let Ok(input) = core::str::from_utf8(&self.buffer[0..self.buffer_pointer]) else {
            println!("Failed to parse the input");
            self.reset_buffer();
            return;
        };
        let mut command_list = input.split_whitespace();
        let Some(command) = command_list.next() else {
            self.reset_buffer();
            return;
        };
        if let Some((_, f)) = Self::COMMAND_LIST.iter().find(|&&(c, _)| c == command) {
            f(command_list);
        } else {
            println!("{} is not defined", command);
        }
        self.reset_buffer();
    }

    pub fn reset_buffer(&mut self) {
        self.buffer_pointer = 0;
        print!("Command>");
    }

    /* 各コマンドの実装 */

    pub fn echo(list: SplitWhitespace) {
        for arg in list {
            print!("{} ", arg);
        }
        println!();
    }

    pub fn power_off(_: SplitWhitespace) {
        println!("The host machine will shutdown!");
        crate::psci::system_off()
    }
}
