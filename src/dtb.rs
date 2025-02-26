#[repr(C)]
struct FdtHeader {
    magic: u32,
    total_size: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_reserved_map: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

pub struct Dtb {
    header: *const FdtHeader,
}

pub struct DtbNode {
    /* node's name の直後の位置 */
    address: usize,
    address_cells: u32,
    size_cells: u32,
}

pub struct DtbProperty {
    address: usize,
    address_cells: u32,
    size_cells: u32,
    len: u32,
}

impl Dtb {
    const MAGIC: u32 = 0xd00dfeed;
    const VERSION: u32 = 17;
    const FDT_TOKEN_BYTE: usize = 0x04;
    const FDT_BEGIN_NODE: [u8; Self::FDT_TOKEN_BYTE] = [0x00, 0x00, 0x00, 0x01];
    const FDT_END_NODE: [u8; Self::FDT_TOKEN_BYTE] = [0x00, 0x00, 0x00, 0x02];
    const FDT_PROP: [u8; Self::FDT_TOKEN_BYTE] = [0x00, 0x00, 0x00, 0x03];
    const FDT_NOP: [u8; Self::FDT_TOKEN_BYTE] = [0x00, 0x00, 0x00, 0x04];
    const FDT_END: [u8; Self::FDT_TOKEN_BYTE] = [0x00, 0x00, 0x00, 0x09];

    const PROP_ADDRESS_CELLS: [u8; 14] = *b"#address-cells";
    const PROP_SIZE_CELLS: [u8; 11] = *b"#size-cells";
    const PROP_REG: [u8; 3] = *b"reg";
    const PROP_STATUS: [u8; 6] = *b"status";
    const PROP_STATUS_OKAY: [u8; 5] = *b"okay\0";
    const PROP_COMPATIBLE: [u8; 10] = *b"compatible";
    pub const PROP_INTERRUPTS: [u8; 10] = *b"interrupts";

    const DEFAULT_ADDRESS_CELLS: u32 = 2;
    const DEFAULT_SIZE_CELLS: u32 = 1;

    pub fn new(dtb_address: usize) -> Result<Self, ()> {
        let fdt_header = unsafe { &*(dtb_address as *const FdtHeader) };
        if u32::from_be(fdt_header.magic) != Self::MAGIC {
            return Err(());
        }
        if u32::from_be(fdt_header.version) > Self::VERSION {
            return Err(());
        }
        Ok(Self { header: fdt_header })
    }

    pub fn get_total_size(&self) -> usize {
        u32::from_be(unsafe { &*(self.header) }.total_size) as usize
    }

    fn compare_name_segment(
        &self,
        name_offset: u32,
        name: &[u8],
        delimiter: &[u8],
    ) -> Result<bool, ()> {
        let name_offset = name_offset as usize;
        if name_offset >= self.get_string_size() {
            return Err(());
        }

        let mut p = self.get_string_offset() + name_offset;
        for c in name {
            if *c != unsafe { *(p as *const u8) } {
                return Ok(false);
            }
            p += 1;
        }
        let l = unsafe { *(p as *const u8) };
        for e in delimiter.iter().chain(b"\0") {
            if *e == l {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn compare_string(
        &self,
        pointer: &mut usize,
        name: &[u8],
        delimiter: &[u8],
    ) -> Result<bool, ()> {
        for c in name {
            if *c != unsafe { *(*pointer as *const u8) } {
                while unsafe { *(*pointer as *const u8) } != b'\0' {
                    *pointer += 1;
                }
                *pointer += 1;
                self.skip_padding(pointer);
                return Ok(false);
            }
            *pointer += 1;
        }
        let l = unsafe { *(*pointer as *const u8) };
        for e in delimiter.iter().chain(b"\0") {
            if *e == l {
                while unsafe { *(*pointer as *const u8) } != b'\0' {
                    *pointer += 1;
                }
                *pointer += 1;
                self.skip_padding(pointer);
                return Ok(true);
            }
        }
        while unsafe { *(*pointer as *const u8) } != b'\0' {
            *pointer += 1;
        }
        *pointer += 1;
        self.skip_padding(pointer);
        Ok(false)
    }

    fn get_struct_offset(&self) -> usize {
        self.header as *const _ as usize
            + u32::from_be(unsafe { &*self.header }.off_dt_struct) as usize
    }

    fn get_struct_size(&self) -> usize {
        u32::from_be(unsafe { &*self.header }.size_dt_struct) as usize
    }

    fn get_string_offset(&self) -> usize {
        self.header as *const _ as usize
            + u32::from_be(unsafe { &*self.header }.off_dt_strings) as usize
    }

    fn get_string_size(&self) -> usize {
        u32::from_be(unsafe { &*self.header }.size_dt_strings) as usize
    }

    fn read_node(&self, address: usize) -> Result<&[u8; Self::FDT_TOKEN_BYTE], ()> {
        if address >= self.get_struct_offset() + self.get_struct_size() {
            Err(())
        } else {
            Ok(unsafe { &*(address as *const [u8; Self::FDT_TOKEN_BYTE]) })
        }
    }

    fn skip_nop(&self, pointer: &mut usize) -> Result<(), ()> {
        while *self.read_node(*pointer)? == Self::FDT_NOP {
            *pointer += Self::FDT_TOKEN_BYTE;
        }
        Ok(())
    }

    fn skip_padding(&self, pointer: &mut usize) {
        *pointer = ((*pointer - 1) & !(Self::FDT_TOKEN_BYTE - 1)) + Self::FDT_TOKEN_BYTE;
    }

    fn _skip_to_next_node(&self, pointer: &mut usize) -> Result<(), ()> {
        loop {
            self.skip_padding(pointer);
            self.skip_nop(pointer)?;
            match *self.read_node(*pointer)? {
                Self::FDT_BEGIN_NODE => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    self._skip_to_next_node(pointer)?;
                }
                Self::FDT_END => {
                    return Err(());
                }
                Self::FDT_END_NODE => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    return Ok(());
                }
                Self::FDT_PROP => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    let len = u32::from_be_bytes(*self.read_node(*pointer)?);
                    *pointer += size_of::<u32>();
                    /* Skip Name Segment */
                    *pointer += size_of::<u32>();
                    *pointer += len as usize;
                }
                _ => {
                    return Err(());
                }
            }
        }
    }

    fn check_address_and_size_cells(
        &self,
        name_segment: u32,
        pointer: usize,
        address_cells: &mut u32,
        size_cells: &mut u32,
    ) -> Result<(), ()> {
        if self.compare_name_segment(name_segment, &Self::PROP_ADDRESS_CELLS, &[])? {
            *address_cells = u32::from_be_bytes(*self.read_node(pointer)?);
        } else if self.compare_name_segment(name_segment, &Self::PROP_SIZE_CELLS, &[])? {
            *size_cells = u32::from_be_bytes(*self.read_node(pointer)?);
        }
        Ok(())
    }

    fn _search_node(
        &self,
        node_name: &[u8],
        pointer: &mut usize,
        mut address_cells: u32,
        mut size_cells: u32,
    ) -> Result<Option<DtbNode>, ()> {
        self.skip_nop(pointer)?;
        if *self.read_node(*pointer)? != Self::FDT_BEGIN_NODE {
            return Err(());
        }
        *pointer += Self::FDT_TOKEN_BYTE;
        if self.compare_string(pointer, node_name, b"@")? {
            return Ok(Some(DtbNode {
                address: *pointer,
                address_cells,
                size_cells,
            }));
        }
        loop {
            self.skip_padding(pointer);
            self.skip_nop(pointer)?;
            match *self.read_node(*pointer)? {
                Self::FDT_BEGIN_NODE => {
                    if let Some(i) =
                        self._search_node(node_name, pointer, address_cells, size_cells)?
                    {
                        return Ok(Some(i));
                    }
                }
                Self::FDT_END => {
                    return Err(());
                }
                Self::FDT_END_NODE => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    return Ok(None);
                }
                Self::FDT_PROP => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    let len = u32::from_be_bytes(*self.read_node(*pointer)?);
                    *pointer += size_of::<u32>();
                    let name_segment = u32::from_be_bytes(*self.read_node(*pointer)?);
                    *pointer += size_of::<u32>();
                    self.check_address_and_size_cells(
                        name_segment,
                        *pointer,
                        &mut address_cells,
                        &mut size_cells,
                    )?;
                    *pointer += len as usize;
                }
                _ => {
                    return Err(());
                }
            }
        }
    }

    pub fn search_node(&self, node_name: &[u8], current_node: Option<&DtbNode>) -> Option<DtbNode> {
        let mut pointer;
        let address_cells;
        let size_cells;

        if let Some(c) = current_node {
            pointer = c.address;
            if self._skip_to_next_node(&mut pointer).is_err() {
                return None;
            }
            address_cells = c.address_cells;
            size_cells = c.size_cells;
        } else {
            pointer = self.get_struct_offset();
            address_cells = Self::DEFAULT_ADDRESS_CELLS;
            size_cells = Self::DEFAULT_SIZE_CELLS;
        };
        while self.read_node(pointer).is_ok() {
            match self._search_node(node_name, &mut pointer, address_cells, size_cells) {
                Ok(Some(n)) => return Some(n),
                Ok(None) => pointer += Self::FDT_TOKEN_BYTE,
                Err(()) => return None,
            }
        }
        None
    }

    fn _search_node_by_compatible(
        &self,
        compatible: &[u8],
        pointer: &mut usize,
        mut address_cells: u32,
        mut size_cells: u32,
    ) -> Result<Option<DtbNode>, ()> {
        self.skip_nop(pointer)?;
        if *self.read_node(*pointer)? != Self::FDT_BEGIN_NODE {
            return Err(());
        }
        *pointer += Self::FDT_TOKEN_BYTE;
        while unsafe { *(*pointer as *const u8) } != b'\0' {
            *pointer += 1;
        }
        *pointer += 1;
        self.skip_padding(pointer);

        let temporary_pointer = *pointer;

        loop {
            self.skip_padding(pointer);
            self.skip_nop(pointer)?;
            match *self.read_node(*pointer)? {
                Self::FDT_BEGIN_NODE => {
                    if let Some(i) = self._search_node_by_compatible(
                        compatible,
                        pointer,
                        address_cells,
                        size_cells,
                    )? {
                        return Ok(Some(i));
                    }
                }

                Self::FDT_END_NODE => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    return Ok(None);
                }
                Self::FDT_PROP => {
                    *pointer += Self::FDT_TOKEN_BYTE;
                    let len = u32::from_be_bytes(*self.read_node(*pointer)?);
                    *pointer += size_of::<u32>();
                    let name_segment = u32::from_be_bytes(*self.read_node(*pointer)?);
                    *pointer += size_of::<u32>();
                    self.check_address_and_size_cells(
                        name_segment,
                        *pointer,
                        &mut address_cells,
                        &mut size_cells,
                    )?;
                    if self.compare_name_segment(name_segment, &Self::PROP_COMPATIBLE, &[])? {
                        let compatible_prop = DtbProperty {
                            address: *pointer,
                            address_cells,
                            size_cells,
                            len,
                        };
                        if self._is_device_compatible(&compatible_prop, compatible) {
                            return Ok(Some(DtbNode {
                                address: temporary_pointer,
                                address_cells,
                                size_cells,
                            }));
                        }
                    }
                    *pointer += len as usize;
                }
                Self::FDT_END => {
                    return Err(());
                }
                _ => {
                    return Err(());
                }
            }
        }
    }

    pub fn search_node_by_compatible(
        &self,
        compatible: &[u8],
        current_node: Option<&DtbNode>,
    ) -> Option<DtbNode> {
        let mut pointer;
        let address_cells;
        let size_cells;

        if let Some(c) = current_node {
            pointer = c.address;
            if self._skip_to_next_node(&mut pointer).is_err() {
                return None;
            }
            address_cells = c.address_cells;
            size_cells = c.size_cells;
        } else {
            pointer = self.get_struct_offset();
            address_cells = Self::DEFAULT_ADDRESS_CELLS;
            size_cells = Self::DEFAULT_SIZE_CELLS;
        };
        while self.read_node(pointer).is_ok() {
            match self._search_node_by_compatible(
                compatible,
                &mut pointer,
                address_cells,
                size_cells,
            ) {
                Ok(Some(n)) => return Some(n),
                Ok(None) => pointer += Self::FDT_TOKEN_BYTE,
                Err(()) => return None,
            }
        }
        None
    }

    pub fn get_property(&self, node: &DtbNode, property_name: &[u8]) -> Option<DtbProperty> {
        let mut p = node.address;
        let mut address_cells = node.address_cells;
        let mut size_cells = node.size_cells;
        loop {
            self.skip_padding(&mut p);
            if self.skip_nop(&mut p).is_err() {
                return None;
            }
            match *self.read_node(p).ok()? {
                Self::FDT_BEGIN_NODE => {
                    return None;
                }
                Self::FDT_END => {
                    return None;
                }
                Self::FDT_END_NODE => {
                    return None;
                }
                Self::FDT_PROP => {
                    p += Self::FDT_TOKEN_BYTE;
                    let len = u32::from_be_bytes(*self.read_node(p).ok()?);
                    p += size_of::<u32>();
                    let name_segment = u32::from_be_bytes(*self.read_node(p).ok()?);
                    p += size_of::<u32>();
                    self.check_address_and_size_cells(
                        name_segment,
                        p,
                        &mut address_cells,
                        &mut size_cells,
                    )
                    .ok()?;
                    if self
                        .compare_name_segment(name_segment, property_name, &[])
                        .ok()?
                    {
                        return Some(DtbProperty {
                            address: p,
                            address_cells,
                            size_cells,
                            len,
                        });
                    }
                    p += len as usize;
                }
                _ => {
                    return None;
                }
            }
        }
    }

    pub fn is_node_operational(&self, node: &DtbNode) -> bool {
        self.get_property(node, &Self::PROP_STATUS)
            .map(|p| unsafe { *(p.address as *const [u8; 5]) } == Self::PROP_STATUS_OKAY)
            .unwrap_or(true)
    }

    fn _is_device_compatible(&self, info: &DtbProperty, compatible: &[u8]) -> bool {
        let mut p = 0;
        let mut skip = false;
        'outer: while p < info.len {
            if skip {
                if unsafe { *((info.address + (p as usize)) as *const u8) } == b'\0' {
                    skip = false;
                }
                p += 1;
                continue;
            }
            for c in compatible.iter().chain(b"\0") {
                if unsafe { *((info.address + (p as usize)) as *const u8) } != *c {
                    skip = true;
                    continue 'outer;
                }
                p += 1;
            }
            return true;
        }
        false
    }

    pub fn is_device_compatible(&self, node: &DtbNode, compatible: &[u8]) -> bool {
        let Some(info) = self.get_property(node, &Self::PROP_COMPATIBLE) else {
            return false;
        };
        self._is_device_compatible(&info, compatible)
    }

    pub fn read_reg_property(&self, node: &DtbNode, index: usize) -> Option<(usize, usize)> {
        let info = self.get_property(node, &Self::PROP_REG)?;
        let mut address: usize = 0;
        let mut size: usize = 0;
        let offset = ((info.address_cells + info.size_cells) as usize) * 4 * index;
        if offset + ((info.address_cells + info.size_cells) as usize) * 4 > info.len as usize {
            return None;
        }
        for i in 0..(info.address_cells * 4) {
            address <<= 8;
            address |= unsafe { *((info.address + offset + i as usize) as *const u8) } as usize;
        }
        for i in 0..(info.size_cells * 4) {
            size <<= 8;
            size |= unsafe {
                *((info.address + offset + (info.address_cells * 4 + i) as usize) as *const u8)
            } as usize;
        }
        Some((address, size))
    }

    pub fn read_property_as_u8_array(&self, info: &DtbProperty) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                info.address as *const u8,
                (info.len as usize) / size_of::<u8>(),
            )
        }
    }

    pub fn read_property_as_u32_array(&self, info: &DtbProperty) -> &[u32] {
        unsafe {
            core::slice::from_raw_parts(
                info.address as *const u32,
                (info.len as usize) / size_of::<u32>(),
            )
        }
    }

    pub fn read_property_as_u32(&self, info: &DtbProperty) -> Option<u32> {
        if (info.len as usize) < size_of::<u32>() {
            None
        } else {
            Some(unsafe { *(info.address as *const u32) })
        }
    }
}
