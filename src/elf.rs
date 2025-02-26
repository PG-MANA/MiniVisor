//!
//! ELFヘッダに関する実装
//!

const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];
const ELF_CLASS: u8 = 0x02;
const ELF_HEADER_VERSION: u8 = 0x01;
const ELF_SUPPORTED_VERSION: u32 = 1;

pub const ELF_PROGRAM_HEADER_SEGMENT_LOAD: u32 = 0x01;

#[repr(C)]
pub struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
pub struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub struct Elf64ProgramHeaderIter {
    pointer: usize,
    size: u16,
    remaining: u16,
}

impl Elf64Header {
    pub fn new(address: usize) -> Result<&'static Self, ()> {
        let s = unsafe { &*(address as *const Self) };
        if s.e_ident[0..4] != ELF_MAGIC
            || s.e_ident[4] != ELF_CLASS
            || s.e_ident[6] != ELF_HEADER_VERSION
            || s.e_version != ELF_SUPPORTED_VERSION
        {
            return Err(());
        }
        Ok(s)
    }

    const fn get_num_of_program_header(&self) -> u16 {
        self.e_phnum
    }

    pub const fn get_program_header_offset(&self) -> u64 {
        self.e_phoff
    }

    const fn get_program_header_entry_size(&self) -> u16 {
        self.e_phentsize
    }

    pub fn get_program_headers(&self) -> Elf64ProgramHeaderIter {
        Elf64ProgramHeaderIter {
            pointer: self as *const _ as usize + self.get_program_header_offset() as usize,
            size: self.get_program_header_entry_size(),
            remaining: self.get_num_of_program_header(),
        }
    }
}

impl Iterator for Elf64ProgramHeaderIter {
    type Item = &'static Elf64ProgramHeader;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            let r = unsafe { &*(self.pointer as *const Elf64ProgramHeader) };
            self.pointer += self.size as usize;
            self.remaining -= 1;
            Some(r)
        }
    }
}

impl Elf64ProgramHeader {
    pub const fn get_segment_type(&self) -> u32 {
        self.p_type
    }

    pub const fn get_physical_address(&self) -> u64 {
        self.p_paddr
    }

    pub const fn get_memory_size(&self) -> u64 {
        self.p_memsz
    }
}
