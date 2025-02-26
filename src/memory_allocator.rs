//!
//! Memory Allocator
//!

use crate::paging::PAGE_SHIFT;

use core::cell::UnsafeCell;

pub struct MemoryAllocator {
    free_size: usize,
    first_entry: Option<u32>,
    free_list: [Option<u32>; Self::NUM_OF_FREE_LIST],
    memory_entry_pool: MemoryEntryPool,
}

type MemoryEntryPool = UnsafeCell<[MemoryEntry; MemoryAllocator::POOL_SIZE]>;

#[derive(Clone, Copy)]
struct MemoryEntry {
    previous: Option<u32>,
    next: Option<u32>,
    list_prev: Option<u32>,
    list_next: Option<u32>,
    start: usize,
    end: usize,
    enabled: bool,
    id: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryError {
    NoEntry,
    InvalidEntry,
    NoMemory,
    InvalidRequest,
}

impl MemoryAllocator {
    const POOL_SIZE: usize = 64;
    const NUM_OF_FREE_LIST: usize = 12;

    pub const fn new() -> Self {
        Self {
            free_size: 0,
            free_list: [None; Self::NUM_OF_FREE_LIST],
            memory_entry_pool: UnsafeCell::new([MemoryEntry::new(); Self::POOL_SIZE]),
            first_entry: None,
        }
    }

    fn create_memory_entry(
        pool: &MemoryEntryPool,
    ) -> Result<&'static mut MemoryEntry, MemoryError> {
        for (i, e) in unsafe { &mut *pool.get() }.iter_mut().enumerate() {
            if !e.enabled {
                *e = MemoryEntry::new();
                e.id = i as u32;
                e.enabled = true;
                return Ok(e);
            }
        }
        Err(MemoryError::NoEntry)
    }

    fn get_memory_entry(
        pool: &MemoryEntryPool,
        index: u32,
    ) -> Result<&'static mut MemoryEntry, MemoryError> {
        let index = index as usize;
        if index >= Self::POOL_SIZE {
            Err(MemoryError::InvalidEntry)
        } else {
            Ok(unsafe { &mut (*pool.get())[index] })
        }
    }

    fn search_entry_containing_address_mut(
        pool: &MemoryEntryPool,
        first_entry: Option<u32>,
        address: usize,
    ) -> Option<&mut MemoryEntry> {
        let first_entry = first_entry?;
        let mut entry = Self::get_memory_entry(pool, first_entry).ok()?;
        while entry.get_start_address() < address && entry.get_end_address() < address {
            if let Some(t) = Self::get_next_entry(pool, entry) {
                entry = t;
            } else {
                return None;
            }
        }
        if address >= entry.get_start_address() && address <= entry.get_end_address() {
            Some(entry)
        } else {
            None
        }
    }

    fn search_entry_previous_address_mut(
        pool: &MemoryEntryPool,
        first_entry: Option<u32>,
        address: usize,
    ) -> Option<&mut MemoryEntry> {
        let first_entry = first_entry?;
        let mut entry = Self::get_memory_entry(pool, first_entry).ok()?;
        while entry.get_start_address() < address {
            if let Some(t) = Self::get_next_entry(pool, entry) {
                entry = t;
            } else {
                return if entry.get_end_address() <= address {
                    Some(entry)
                } else {
                    Self::get_prev_entry(pool, entry)
                };
            }
        }
        Self::get_prev_entry(pool, entry)
    }

    fn define_used_memory(
        &mut self,
        start: usize,
        size: usize,
        align_order: usize,
        target_entry: &mut Option<&mut MemoryEntry>,
    ) -> Result<(), MemoryError> {
        if size == 0 {
            return Err(MemoryError::InvalidRequest);
        } else if self.free_size < size {
            return Err(MemoryError::NoMemory);
        }
        if align_order != 0 {
            let (aligned_start, aligned_size) =
                Self::align_address_and_size(start, size, align_order);
            return self.define_used_memory(aligned_start, aligned_size, 0, target_entry);
        }
        let entry = if let Some(t) = target_entry {
            t
        } else if let Some(t) = Self::search_entry_containing_address_mut(
            &self.memory_entry_pool,
            self.first_entry,
            start,
        ) {
            t
        } else {
            return Err(MemoryError::InvalidRequest);
        };
        if entry.get_start_address() == start {
            if entry.get_end_address() == get_end_address(start, size) {
                /* Delete the entry */
                if entry.is_first_entry() {
                    if let Some(next) = Self::get_next_entry(&self.memory_entry_pool, entry) {
                        self.first_entry = Some(next.id);
                    } else {
                        return Err(MemoryError::NoEntry);
                    }
                }
                Self::unchain_entry_from_free_list(
                    &self.memory_entry_pool,
                    &mut self.free_list,
                    entry,
                );
                entry.delete(&self.memory_entry_pool);
                if target_entry.is_some() {
                    *target_entry = None;
                }
            } else {
                let old_size = entry.get_size();
                entry.set_range(start + size, entry.get_end_address());
                Self::chain_entry_to_free_list(
                    &self.memory_entry_pool,
                    &mut self.free_list,
                    entry,
                    Some(old_size),
                );
            }
        } else if entry.get_end_address() == start {
            if size != 1 {
                return Err(MemoryError::InvalidRequest);
            }
            /* Allocate 1 byte of end_address */
            entry.set_range(entry.get_start_address(), start - 1);
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                entry,
                Some(entry.get_size() + 1),
            );
        } else if entry.get_end_address() == get_end_address(start, size) {
            let old_size = entry.get_size();
            entry.set_range(entry.get_start_address(), start - 1);
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                entry,
                Some(old_size),
            );
        } else {
            let new_entry = Self::create_memory_entry(&self.memory_entry_pool)?;
            let old_size = entry.get_size();
            new_entry.set_range(start + size, entry.get_end_address());
            entry.set_range(entry.get_start_address(), start - 1);
            if let Some(next) = Self::get_next_entry(&self.memory_entry_pool, entry) {
                new_entry.chain_after_me(next);
            }
            entry.chain_after_me(new_entry);
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                entry,
                Some(old_size),
            );
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                new_entry,
                None,
            );
        }
        self.free_size -= size;
        Ok(())
    }

    fn define_free_memory(&mut self, start: usize, size: usize) -> Result<(), MemoryError> {
        if size == 0 {
            return Err(MemoryError::InvalidRequest);
        }
        let entry = if let Some(e) = Self::search_entry_previous_address_mut(
            &self.memory_entry_pool,
            self.first_entry,
            start,
        ) {
            e
        } else if let Ok(e) =
            Self::get_memory_entry(&self.memory_entry_pool, self.first_entry.unwrap())
        {
            e
        } else {
            return Err(MemoryError::InvalidEntry);
        };
        let end = get_end_address(start, size);

        if entry.get_start_address() <= start && entry.get_end_address() >= end {
            /* already freed */
            return Err(MemoryError::InvalidRequest);
        } else if entry.get_end_address() >= start && !entry.is_first_entry() {
            /* Free duplicated area */
            return self.define_free_memory(
                entry.get_end_address() + 1,
                end - entry.get_end_address() + 2,
            );
        } else if entry.get_end_address() == end {
            /* Free duplicated area */
            /* entry may be first entry */
            return self.define_free_memory(start, size - entry.get_size());
        }

        let mut processed = false;
        let old_size = entry.get_size();
        let address_after_entry = entry.get_end_address() + 1;

        if address_after_entry == start {
            entry.set_range(entry.get_start_address(), end);
            processed = true;
        }

        if entry.is_first_entry() && entry.get_start_address() == end + 1 {
            entry.set_range(start, entry.get_end_address());
            processed = true;
        }

        if let Some(next) = Self::get_next_entry(&self.memory_entry_pool, entry) {
            if next.get_start_address() <= start {
                assert!(!processed);
                return if next.get_end_address() >= end {
                    Err(MemoryError::InvalidRequest) /* already freed */
                } else {
                    self.define_free_memory(
                        next.get_end_address() + 1,
                        end - next.get_end_address(),
                    )
                };
            }
            if next.get_start_address() == end + 1 {
                let next_old_size = next.get_size();
                next.set_range(start, next.get_end_address());
                Self::chain_entry_to_free_list(
                    &self.memory_entry_pool,
                    &mut self.free_list,
                    next,
                    Some(next_old_size),
                );
                processed = true;
            }

            if (next.get_start_address() == entry.get_end_address() + 1)
                || (processed && address_after_entry >= next.get_start_address())
            {
                entry.set_range(
                    entry.get_start_address(),
                    entry.get_end_address().max(next.get_end_address()),
                );

                Self::unchain_entry_from_free_list(
                    &self.memory_entry_pool,
                    &mut self.free_list,
                    next,
                );
                next.delete(&self.memory_entry_pool);
            }
            if processed {
                self.free_size += size;
                Self::chain_entry_to_free_list(
                    &self.memory_entry_pool,
                    &mut self.free_list,
                    entry,
                    Some(old_size),
                );
                return Ok(());
            }
            let new_entry = Self::create_memory_entry(&self.memory_entry_pool)?;
            new_entry.set_range(start, end);
            if new_entry.get_end_address() < entry.get_start_address() {
                if let Some(prev_entry) = Self::get_prev_entry(&self.memory_entry_pool, entry) {
                    assert!(prev_entry.get_end_address() < new_entry.get_start_address());
                    prev_entry.chain_after_me(new_entry);
                    new_entry.chain_after_me(entry);
                } else {
                    self.first_entry = Some(new_entry.id);
                    new_entry.chain_after_me(entry);
                }
            } else {
                next.set_prev_entry(new_entry);
                new_entry.set_next_entry(next);
                entry.chain_after_me(new_entry);
            }
            self.free_size += size;
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                entry,
                Some(old_size),
            );
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                new_entry,
                None,
            );
            Ok(())
        } else {
            if processed {
                self.free_size += size;
                Self::chain_entry_to_free_list(
                    &self.memory_entry_pool,
                    &mut self.free_list,
                    entry,
                    Some(old_size),
                );
                return Ok(());
            }
            let new_entry = Self::create_memory_entry(&self.memory_entry_pool)?;
            new_entry.set_range(start, end);
            if entry.get_end_address() < new_entry.get_start_address() {
                entry.chain_after_me(new_entry);
            } else {
                if let Some(prev_entry) = Self::get_prev_entry(&self.memory_entry_pool, entry) {
                    assert!(prev_entry.get_end_address() < entry.get_start_address());
                    prev_entry.chain_after_me(new_entry);
                } else {
                    self.first_entry = Some(new_entry.id);
                }
                new_entry.chain_after_me(entry);
            }
            self.free_size += size;
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                entry,
                Some(old_size),
            );
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                new_entry,
                None,
            );
            Ok(())
        }
    }

    pub fn allocate(&mut self, size: usize, align_order: usize) -> Result<usize, MemoryError> {
        if size == 0 {
            return Err(MemoryError::InvalidRequest);
        } else if self.free_size <= size {
            return Err(MemoryError::NoMemory);
        }
        let page_order = Self::size_to_page_order(size);
        for i in page_order..Self::NUM_OF_FREE_LIST {
            let first_entry = if let Some(t) = self.free_list[i] {
                Self::get_memory_entry(&self.memory_entry_pool, t)?
            } else {
                continue;
            };

            let mut entry = Some(first_entry);
            while let Some(e) = entry {
                if e.get_size() >= size {
                    let address_to_allocate = if align_order != 0 {
                        let (aligned_address, aligned_available_size) =
                            Self::align_address_and_available_size(
                                e.get_start_address(),
                                e.get_size(),
                                align_order,
                            );
                        if aligned_available_size < size {
                            entry = e.list_next.and_then(|n| {
                                Self::get_memory_entry(&self.memory_entry_pool, n).ok()
                            });
                            continue;
                        }
                        aligned_address
                    } else {
                        e.get_start_address()
                    };
                    self.define_used_memory(address_to_allocate, size, 0, &mut Some(e))?;
                    return Ok(address_to_allocate);
                }
                entry = e
                    .list_next
                    .and_then(|n| Self::get_memory_entry(&self.memory_entry_pool, n).ok());
            }
        }
        Err(MemoryError::NoMemory)
    }

    pub fn reserve_memory(
        &mut self,
        start_address: usize,
        size: usize,
        align_order: usize,
    ) -> Result<(), MemoryError> {
        self.define_used_memory(start_address, size, align_order, &mut None)
    }

    pub fn free(&mut self, start: usize, size: usize) -> Result<(), MemoryError> {
        if self.free_size == 0 {
            let first_entry = Self::create_memory_entry(&self.memory_entry_pool)?;
            first_entry.set_range(start, get_end_address(start, size));
            Self::chain_entry_to_free_list(
                &self.memory_entry_pool,
                &mut self.free_list,
                first_entry,
                None,
            );
            self.first_entry = Some(first_entry.id);
            self.free_size = size;
        } else {
            self.define_free_memory(start, size)?;
            self.free_size += size;
        }
        Ok(())
    }

    fn unchain_entry_from_free_list(
        pool: &MemoryEntryPool,
        free_list: &mut [Option<u32>; Self::NUM_OF_FREE_LIST],
        entry: &mut MemoryEntry,
    ) {
        let order = Self::size_to_page_order(entry.get_size());
        if free_list[order] == Some(entry.id) {
            free_list[order] = entry.list_next;
        }
        entry.unchain_from_freelist(pool);
    }

    fn chain_entry_to_free_list(
        pool: &MemoryEntryPool,
        free_list: &mut [Option<u32>; Self::NUM_OF_FREE_LIST],
        entry: &mut MemoryEntry,
        old_size: Option<usize>,
    ) {
        let new_order = Self::size_to_page_order(entry.get_size());
        if let Some(old_size) = old_size {
            if old_size == entry.get_size() {
                return;
            }
            let old_order = Self::size_to_page_order(old_size);
            if free_list[old_order] == Some(entry.id) {
                free_list[old_order] = entry.list_next;
            }
            entry.unchain_from_freelist(pool);
        }
        assert_eq!(entry.list_next, None);
        assert_eq!(entry.list_prev, None);

        if let Some(mut list_entry) =
            free_list[new_order].and_then(|i| Self::get_memory_entry(pool, i).ok())
        {
            if list_entry.get_size() >= entry.get_size() {
                list_entry.list_prev = Some(entry.id);
                entry.list_next = Some(list_entry.id);
                free_list[new_order] = Some(entry.id);
            } else {
                loop {
                    if let Some(next_entry) = list_entry
                        .list_next
                        .and_then(|n| Self::get_memory_entry(pool, n).ok())
                    {
                        if next_entry.get_size() >= entry.get_size() {
                            list_entry.list_next = Some(entry.id);
                            entry.list_prev = Some(list_entry.id);
                            entry.list_next = Some(next_entry.id);
                            next_entry.list_prev = Some(entry.id);
                            break;
                        }
                        list_entry = next_entry;
                    } else {
                        list_entry.list_next = Some(entry.id);
                        entry.list_prev = Some(list_entry.id);
                        break;
                    }
                }
            }
        } else {
            free_list[new_order] = Some(entry.id);
        }
    }

    fn get_next_entry(
        pool: &MemoryEntryPool,
        entry: &MemoryEntry,
    ) -> Option<&'static mut MemoryEntry> {
        entry
            .next
            .and_then(|n| Self::get_memory_entry(pool, n).ok())
    }

    fn get_prev_entry(
        pool: &MemoryEntryPool,
        entry: &MemoryEntry,
    ) -> Option<&'static mut MemoryEntry> {
        entry
            .previous
            .and_then(|n| Self::get_memory_entry(pool, n).ok())
    }

    #[inline]
    const fn size_to_page_order(size: usize) -> usize {
        let mut order = 0;
        while size > (1 << (order + PAGE_SHIFT)) {
            order += 1;
            if order == Self::NUM_OF_FREE_LIST - 1 {
                return order;
            }
        }
        order
    }

    #[inline]
    const fn align_address_and_size(
        address: usize,
        size: usize,
        align_order: usize,
    ) -> (usize /* address */, usize /* size */) {
        let align_size = 1 << align_order;
        let mask = !(align_size - 1);
        let aligned_address = address & mask;
        (
            aligned_address,
            ((size + (address - aligned_address) - 1) & mask) + align_size,
        )
    }

    #[inline]
    const fn align_address_and_available_size(
        start: usize,
        size: usize,
        align_order: usize,
    ) -> (usize, usize) {
        if start == 0 {
            (0, size)
        } else {
            let align_size = 1 << align_order;
            let mask = !(align_size - 1);
            let aligned_address = ((start - 1) & mask) + align_size;
            if size > (aligned_address - start) {
                (aligned_address, size - (aligned_address - start))
            } else {
                (aligned_address, 0)
            }
        }
    }
}

impl MemoryEntry {
    const fn new() -> Self {
        Self {
            previous: None,
            next: None,
            list_prev: None,
            list_next: None,
            start: 0,
            end: 0,
            enabled: false,
            id: 0,
        }
    }

    pub fn delete(&mut self, pool: &MemoryEntryPool) {
        if let Some(previous) = MemoryAllocator::get_prev_entry(pool, self) {
            if let Some(next) = MemoryAllocator::get_next_entry(pool, self) {
                previous.chain_after_me(next);
            } else {
                previous.unset_next_entry();
            }
        } else if let Some(next) = MemoryAllocator::get_next_entry(pool, self) {
            next.unset_prev_entry();
        }
        self.previous = None;
        self.next = None;
        self.enabled = false;
    }

    pub fn set_range(&mut self, start: usize, end: usize) {
        assert!(start < end);
        self.start = start;
        self.end = end;
    }

    pub fn get_start_address(&self) -> usize {
        self.start
    }

    pub fn get_end_address(&self) -> usize {
        self.end
    }

    pub fn set_prev_entry(&mut self, prev: &mut Self) {
        self.previous = Some(prev.id);
    }

    pub fn unset_prev_entry(&mut self) {
        self.previous = None;
    }

    pub fn set_next_entry(&mut self, next: &mut Self) {
        self.next = Some(next.id);
    }

    pub fn unset_next_entry(&mut self) {
        self.next = None;
    }

    pub fn get_size(&self) -> usize {
        self.end - self.start + 1
    }

    pub fn chain_after_me(&mut self, entry: &mut Self) {
        self.next = Some(entry.id);
        entry.previous = Some(self.id);
    }

    pub fn is_first_entry(&self) -> bool {
        self.previous.is_none()
    }

    pub fn unchain_from_freelist(&mut self, pool: &MemoryEntryPool) {
        if let Some(prev_entry) = self
            .list_prev
            .and_then(|i| MemoryAllocator::get_memory_entry(pool, i).ok())
        {
            prev_entry.list_next = self.list_next;
        }
        if let Some(next_entry) = self
            .list_next
            .and_then(|i| MemoryAllocator::get_memory_entry(pool, i).ok())
        {
            next_entry.list_prev = self.list_prev;
        }
        self.list_next = None;
        self.list_prev = None;
    }
}

const fn get_end_address(address: usize, size: usize) -> usize {
    address + size - 1
}
