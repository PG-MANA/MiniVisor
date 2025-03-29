//!
//! FAT32の実装
//!

use crate::drivers::virtio_blk::VirtioBlk;
use crate::paging::PAGE_SHIFT;
use crate::{allocate_pages, free_pages};

const FAT32_SIGNATURE: [u8; 8] = [b'F', b'A', b'T', b'3', b'2', b' ', b' ', b' '];

const BYTES_PER_SECTOR_OFFSET: usize = 11;
const SECTORS_PER_CLUSTER_OFFSET: usize = 13;
const NUM_OF_RESERVED_CLUSTER_OFFSET: usize = 14;
const NUM_OF_FATS_OFFSET: usize = 16;
const FAT_SIZE_OFFSET: usize = 36;
const ROOT_CLUSTER_OFFSET: usize = 44;
const FAT32_SIGNATURE_OFFSET: usize = 82;

const FAT32_ATTRIBUTE_DIRECTORY: u8 = 0x10;
const FAT32_ATTRIBUTE_LONG_FILE_NAME: u8 = 0x0F;

pub struct Fat32 {
    base_lba: usize,
    lba_size: usize,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    fat_sectors: u32,
    number_of_fats: u16,
    fat: usize,
    root_directory_list: usize,
}

pub struct FileInfo {
    entry_cluster: u32,
    file_size: u32,
}

#[repr(C)]
struct DirectoryEntry {
    name: [u8; 8],
    name_extension: [u8; 3],
    attribute: u8,
    reserved: [u8; 8],
    starting_cluster_number_high: u16,
    time_recorded: u16,
    date_recorded: u16,
    starting_cluster_number: u16,
    file_length: u32,
}

impl Fat32 {
    pub fn new(blk: &mut VirtioBlk, base_lba: usize, lba_size: usize) -> Result<Fat32, ()> {
        let mut bpb_buffer: [u8; 512] = [0; 512];
        let bpb_address = &mut bpb_buffer as *mut _ as usize;
        blk.read(bpb_address, (base_lba * lba_size) as u64, 512)?;

        if unsafe { *((bpb_address + FAT32_SIGNATURE_OFFSET) as *const [u8; 8]) } != FAT32_SIGNATURE
        {
            println!("Bad Signature");
            return Err(());
        }

        /* BPBの読み取り */
        let bytes_per_sector = unsafe { *((bpb_address + BYTES_PER_SECTOR_OFFSET) as *const u16) };
        let sectors_per_cluster =
            unsafe { *((bpb_address + SECTORS_PER_CLUSTER_OFFSET) as *const u8) };
        let reserved_sectors =
            unsafe { *((bpb_address + NUM_OF_RESERVED_CLUSTER_OFFSET) as *const u16) };
        let number_of_fats = unsafe { *((bpb_address + NUM_OF_FATS_OFFSET) as *const u16) };
        let fat_sectors = unsafe { *((bpb_address + FAT_SIZE_OFFSET) as *const u32) };
        let root_cluster = unsafe { *((bpb_address + ROOT_CLUSTER_OFFSET) as *const u32) };

        /* FATの読み込み */
        let fat_size = (fat_sectors as usize) * (bytes_per_sector as usize);
        let lba_aligned_fat_size = ((fat_size - 1) & (!(lba_size - 1))) + lba_size;
        let fat = allocate_pages(
            (lba_aligned_fat_size >> PAGE_SHIFT) + 1,
            lba_size.ilog2() as usize,
        )
        .expect("Failed to allocate memory");
        let fat_address =
            ((base_lba * lba_size) as u64) + (reserved_sectors as u64) * (bytes_per_sector as u64);
        if blk
            .read(fat, fat_address, lba_aligned_fat_size as u64)
            .is_err()
        {
            free_pages(fat, (lba_aligned_fat_size >> PAGE_SHIFT) + 1);
            return Err(());
        }

        /* ルートディレクトリリストの読み込み */
        let root_directory_pages =
            (((sectors_per_cluster as usize) * (bytes_per_sector as usize)) >> PAGE_SHIFT) + 1;
        let root_directory_list = allocate_pages(root_directory_pages, lba_size.ilog2() as usize)
            .expect("Failed to allocate memory");

        let fat32 = Fat32 {
            base_lba,
            lba_size,
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            fat_sectors,
            number_of_fats,
            fat,
            root_directory_list,
        };

        let root_sector = fat32.cluster_to_sector(root_cluster);
        if fat32
            .read_sectors(
                blk,
                root_directory_list,
                root_sector,
                sectors_per_cluster as u32,
            )
            .is_err()
        {
            free_pages(root_directory_list, root_directory_pages);
            free_pages(fat, (lba_aligned_fat_size >> PAGE_SHIFT) + 1);
            return Err(());
        }

        Ok(fat32)
    }

    fn cluster_to_sector(&self, cluster: u32) -> u32 {
        (self.reserved_sectors as u32)
            + (self.number_of_fats as u32) * self.fat_sectors
            + (cluster - 2) * (self.sectors_per_cluster as u32)
    }

    fn get_next_cluster(&self, cluster: u32) -> Option<u32> {
        let fat = unsafe {
            core::slice::from_raw_parts(
                self.fat as *const u32,
                (self.fat_sectors as usize * self.bytes_per_sector as usize) / size_of::<u32>(),
            )
        };
        let n = fat.get(cluster as usize)?;
        if (2..0x0FFFFFF8).contains(n) {
            Some(*n)
        } else {
            None
        }
    }

    fn read_sectors(
        &self,
        blk: &mut VirtioBlk,
        buffer: usize,
        base_sector: u32,
        sectors: u32,
    ) -> Result<(), ()> {
        blk.read(
            buffer,
            ((self.base_lba * self.lba_size)
                + (base_sector as usize) * (self.bytes_per_sector as usize)) as u64,
            (sectors as u64) * (self.bytes_per_sector as u64),
        )
    }

    fn write_sectors(
        &self,
        blk: &mut VirtioBlk,
        buffer: usize,
        base_sector: u32,
        sectors: u32,
    ) -> Result<(), ()> {
        blk.write(
            buffer,
            ((self.base_lba * self.lba_size)
                + (base_sector as usize) * (self.bytes_per_sector as usize)) as u64,
            (sectors as u64) * (self.bytes_per_sector as u64),
        )
    }

    fn get_file_name<'a>(e: &DirectoryEntry, buffer: &'a mut [u8; 12]) -> Option<&'a mut str> {
        if e.name[0] == 0x05 {
            buffer[0] = 0xE5;
        } else {
            buffer[0] = e.name[0];
        }
        let mut p = 0;
        for n in &e.name[1..] {
            if *n == b' ' {
                continue;
            }
            p += 1;
            buffer[p] = *n;
        }
        p += 1;
        buffer[p] = b'.';
        for n in &e.name_extension {
            if *n == b' ' {
                continue;
            }
            p += 1;
            buffer[p] = *n;
        }
        if buffer[p] == b'.' {
            buffer[p] = 0;
        } else {
            p += 1;
        }
        core::str::from_utf8_mut(&mut buffer[0..p]).ok()
    }

    pub fn list_files(&self) {
        let len = ((self.bytes_per_sector as usize) * self.sectors_per_cluster as usize)
            / size_of::<DirectoryEntry>();
        let entries = unsafe {
            core::slice::from_raw_parts(self.root_directory_list as *const DirectoryEntry, len)
        };
        assert_eq!(size_of::<DirectoryEntry>(), 32);

        for e in entries {
            if (e.attribute & 0x3F) == FAT32_ATTRIBUTE_LONG_FILE_NAME {
                continue;
            }
            if (e.attribute & FAT32_ATTRIBUTE_DIRECTORY) != 0 {
                continue;
            }
            if e.name[0] == 0 {
                break;
            } else if e.name[0] == 0xE5 {
                continue;
            }
            let mut buffer = [0u8; 12];
            if let Some(file_name) = Self::get_file_name(e, &mut buffer) {
                let file_size = e.file_length;
                println!("{}: File Size:{:#X}", file_name, file_size);
            }
        }
    }

    pub fn search_file(&self, target_name: &str) -> Option<FileInfo> {
        let len = ((self.bytes_per_sector as usize) * self.sectors_per_cluster as usize)
            / size_of::<DirectoryEntry>();
        let entries = unsafe {
            core::slice::from_raw_parts(self.root_directory_list as *const DirectoryEntry, len)
        };
        assert_eq!(size_of::<DirectoryEntry>(), 32);

        for e in entries {
            if (e.attribute & 0x3F) == FAT32_ATTRIBUTE_LONG_FILE_NAME {
                continue;
            }
            if (e.attribute & FAT32_ATTRIBUTE_DIRECTORY) != 0 {
                continue;
            }
            if e.name[0] == 0 {
                break;
            } else if e.name[0] == 0xE5 {
                continue;
            }
            let mut buffer = [0u8; 12];
            if let Some(file_name) = Self::get_file_name(e, &mut buffer) {
                let mut is_same = file_name == target_name;
                if !is_same {
                    /* 小文字にして再度比較 */
                    file_name.make_ascii_lowercase();
                    is_same = file_name == target_name;
                }
                if is_same {
                    let entry_cluster = ((e.starting_cluster_number_high as u32) << 16)
                        | (e.starting_cluster_number as u32);
                    let file_size = e.file_length;
                    return Some(FileInfo {
                        entry_cluster,
                        file_size,
                    });
                }
            }
        }
        None
    }

    pub fn read(
        &self,
        file_info: &FileInfo,
        blk: &mut VirtioBlk,
        buffer_address: usize,
        offset: usize,
        mut length: usize,
    ) -> Result<usize, ()> {
        if offset + length > file_info.file_size as usize {
            if offset >= file_info.file_size as usize {
                return Ok(0);
            }
            length = (file_info.file_size as usize) - offset;
        }

        macro_rules! next_cluster {
            ($c:expr) => {
                match self.get_next_cluster($c) {
                    Some(n) => n,
                    None => {
                        println!("Failed to get next cluster");
                        return Err(());
                    }
                }
            };
        }

        let bytes_per_cluster = self.sectors_per_cluster as usize * self.bytes_per_sector as usize;
        let clusters_to_skip = offset / bytes_per_cluster;
        let mut data_offset = offset - clusters_to_skip * bytes_per_cluster;
        let mut reading_cluster = file_info.entry_cluster;
        let mut buffer_pointer = 0usize;

        for _ in 0..clusters_to_skip {
            reading_cluster = next_cluster!(reading_cluster);
        }

        loop {
            let mut sectors = 0;
            let mut read_bytes = 0;
            let mut sector_offset = 0;
            let first_cluster = reading_cluster;
            let mut data_offset_backup = data_offset;

            loop {
                /* 読み飛ばすセクタ数の計算 */
                if data_offset > (self.bytes_per_sector as usize) {
                    sector_offset = (data_offset / self.bytes_per_sector as usize) as u32;
                    data_offset -= (sector_offset as usize) * (self.bytes_per_sector as usize);
                    data_offset_backup = data_offset;
                }
                /* 読み込むサイズが1クラスタ分に満たない場合 */
                if (length - read_bytes + data_offset) <= bytes_per_cluster {
                    sectors += (1
                        + ((length - read_bytes + data_offset).max(1) - 1)
                            / self.bytes_per_sector as usize) as u32;
                    read_bytes += length - read_bytes;
                    break;
                }
                /* 1クラスタ丸々読み込む */
                sectors += self.sectors_per_cluster as u32;
                read_bytes += bytes_per_cluster - data_offset;

                let next_cluster = next_cluster!(reading_cluster);
                if next_cluster != reading_cluster + 1 {
                    /* クラスタが連続していない */
                    break;
                }
                data_offset = 0;
                reading_cluster = next_cluster;
            }
            data_offset = data_offset_backup;

            let aligned_buffer_size = (((sectors as usize) * (self.bytes_per_sector as usize))
                & (!(self.lba_size - 1)))
                + self.lba_size;
            let buffer = if data_offset != 0 {
                allocate_pages((aligned_buffer_size >> PAGE_SHIFT) + 1, 0).or(Err(()))?
            } else {
                buffer_address + buffer_pointer
            };

            let sector = self.cluster_to_sector(first_cluster) + sector_offset;
            if self.read_sectors(blk, buffer, sector, sectors).is_err() {
                if data_offset != 0 {
                    free_pages(buffer, (aligned_buffer_size >> PAGE_SHIFT) + 1);
                }
                return Err(());
            };
            if data_offset != 0 {
                assert_eq!(buffer_pointer, 0);
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        (buffer + data_offset) as *const u8,
                        buffer_address as *mut u8,
                        read_bytes,
                    )
                };
                free_pages(buffer, (aligned_buffer_size >> PAGE_SHIFT) + 1);
                data_offset = 0;
            }
            buffer_pointer += read_bytes;
            if length == buffer_pointer {
                break;
            }
            reading_cluster = next_cluster!(reading_cluster);
        }
        Ok(buffer_pointer)
    }

    pub fn write(
        &self,
        file_info: &FileInfo,
        blk: &mut VirtioBlk,
        buffer_address: usize,
        offset: usize,
        mut length: usize,
    ) -> Result<usize, ()> {
        if offset + length > file_info.file_size as usize {
            if offset >= file_info.file_size as usize {
                return Ok(0);
            }
            length = (file_info.file_size as usize) - offset;
        }

        macro_rules! next_cluster {
            ($c:expr) => {
                match self.get_next_cluster($c) {
                    Some(n) => n,
                    None => {
                        println!("Failed to get next cluster");
                        return Err(());
                    }
                }
            };
        }

        let bytes_per_cluster = self.sectors_per_cluster as usize * self.bytes_per_sector as usize;
        let clusters_to_skip = offset / bytes_per_cluster;
        let mut data_offset = offset - clusters_to_skip * bytes_per_cluster;
        let mut writing_cluster = file_info.entry_cluster;
        let mut buffer_pointer = 0usize;

        for _ in 0..clusters_to_skip {
            writing_cluster = next_cluster!(writing_cluster);
        }

        loop {
            let mut sectors = 0;
            let mut write_bytes = 0;
            let mut sector_offset = 0;
            let first_cluster = writing_cluster;
            let mut data_offset_backup = data_offset;

            loop {
                if length - write_bytes < (self.bytes_per_sector as usize) {
                    println!(
                        "The size to write must be {}-aligned",
                        self.bytes_per_sector
                    );
                    return Err(());
                }
                /* 飛ばすセクタ数の計算 */
                if data_offset > (self.bytes_per_sector as usize) {
                    sector_offset = (data_offset / self.bytes_per_sector as usize) as u32;
                    data_offset -= (sector_offset as usize) * (self.bytes_per_sector as usize);
                    data_offset_backup = data_offset;
                }
                /* 書き込むサイズが1クラスタ分に満たない場合 */
                if (length - write_bytes + data_offset) <= bytes_per_cluster {
                    sectors += (1
                        + ((length - write_bytes + data_offset).max(1) - 1)
                            / self.bytes_per_sector as usize) as u32;
                    write_bytes += length - write_bytes;
                    break;
                }
                /* 1クラスタ丸々書き込む */
                sectors += self.sectors_per_cluster as u32;
                write_bytes += bytes_per_cluster - data_offset;

                let next_cluster = next_cluster!(writing_cluster);
                if next_cluster != writing_cluster + 1 {
                    /* クラスタが連続していない */
                    break;
                }
                data_offset = 0;
                writing_cluster = next_cluster;
            }
            data_offset = data_offset_backup;
            if data_offset != 0 {
                println!(
                    "The size to write must be {}-aligned",
                    self.bytes_per_sector
                );
                return Err(());
            }
            let sector = self.cluster_to_sector(first_cluster) + sector_offset;
            self.write_sectors(blk, buffer_address + buffer_pointer, sector, sectors)?;
            buffer_pointer += write_bytes;
            if length == buffer_pointer {
                break;
            }
            writing_cluster = next_cluster!(writing_cluster);
        }
        Ok(buffer_pointer)
    }
}

impl FileInfo {
    pub fn get_file_size(&self) -> usize {
        self.file_size as usize
    }
}
