//!
//! Console
//!

use core::str::SplitWhitespace;
use core::sync::atomic::Ordering;

pub struct Console {
    buffer: [u8; Self::BUFFER_SIZE],
    buffer_pointer: usize,
    ignore_lf: bool,
}

impl Console {
    const BUFFER_SIZE: usize = 64;
    #[allow(clippy::type_complexity)]
    const COMMAND_LIST: [(&str, fn(SplitWhitespace) -> bool); 4] = [
        ("boot", Self::boot_vm),
        ("switch", Self::switch_vm),
        ("echo", Self::echo),
        ("poweroff", Self::power_off),
    ];

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
            if f(command_list) {
                self.reset_buffer();
            } else {
                /* 自動的にコンソールを無効化 */
                crate::IS_CONSOLE_ACTIVE.fetch_xor(true, Ordering::Relaxed);
            }
        } else {
            println!("{} is not defined", command);
            self.reset_buffer();
        }
    }

    pub fn reset_buffer(&mut self) {
        self.buffer_pointer = 0;
        print!("Command>");
    }

    /* 各コマンドの実装 */

    pub fn echo(list: SplitWhitespace) -> bool {
        for arg in list {
            print!("{} ", arg);
        }
        println!();
        true
    }

    pub fn power_off(_: SplitWhitespace) -> bool {
        println!("The host machine will shutdown!");
        crate::psci::system_off()
    }

    pub fn boot_vm(_: SplitWhitespace) -> bool {
        if crate::launch_cpu() {
            /* Active VM は自動的に切り替わる */
            println!("Booted a new VM");
            false
        } else {
            println!("Failed to boot a VM");
            true
        }
    }

    pub fn switch_vm(mut args: SplitWhitespace) -> bool {
        let Some(arg) = args.next() else {
            println!("Missing vm_id\nUsage: switch vm_id");
            return true;
        };
        let Some(vm_id) = crate::str_to_usize(arg) else {
            println!("\"{arg}\" is not a number");
            return true;
        };
        if crate::vm::switch_active_vm(vm_id) {
            println!("VM{vm_id} is actived");
            false
        } else {
            println!("VM{vm_id} is not available");
            true
        }
    }
}
