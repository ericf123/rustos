use core::ops::{Deref, DerefMut};

use alloc::boxed::Box;
use alloc::fmt;
use core::alloc::{GlobalAlloc, Layout};

use crate::allocator;
use crate::param::*;
use crate::vm::{PhysicalAddr, VirtualAddr};
use crate::ALLOCATOR;

use aarch64::vmsa::*;
use shim::const_assert_size;

extern crate pi;
use pi::common::{IO_BASE, IO_BASE_END};

#[repr(C)]
pub struct Page([u8; PAGE_SIZE]);
const_assert_size!(Page, PAGE_SIZE);

impl Page {
    pub const SIZE: usize = PAGE_SIZE;
    pub const ALIGN: usize = PAGE_SIZE;

    fn layout() -> Layout {
        unsafe { Layout::from_size_align_unchecked(Self::SIZE, Self::ALIGN) }
    }
}

#[repr(C)]
#[repr(align(65536))]
pub struct L2PageTable {
    pub entries: [RawL2Entry; 8192],
}
const_assert_size!(L2PageTable, PAGE_SIZE);

impl L2PageTable {
    /// Returns a new `L2PageTable`
    fn new() -> L2PageTable {
        L2PageTable {
            entries: [RawL2Entry::new(0); 8192]
        }
    }

    /// Returns a `PhysicalAddr` of the pagetable.
    pub fn as_ptr(&self) -> PhysicalAddr {
        PhysicalAddr::from(self as *const L2PageTable)
    }
}

#[derive(Copy, Clone)]
pub struct L3Entry(RawL3Entry);

impl L3Entry {
    /// Returns a new `L3Entry`.
    fn new() -> L3Entry {
        L3Entry(RawL3Entry::new(0))
    }

    /// Returns `true` if the L3Entry is valid and `false` otherwise.
    fn is_valid(&self) -> bool {
        self.0.get_masked(RawL3Entry::VALID) == EntryValid::Valid
    }

    /// Extracts `ADDR` field of the L3Entry and returns as a `PhysicalAddr`
    /// if valid. Otherwise, return `None`.
    fn get_page_addr(&self) -> Option<PhysicalAddr> {
        if self.is_valid() {
            let addr = self.0.get_value(RawL3Entry::ADDR) as usize;
            Some((addr * PAGE_SIZE).into())
        } else {
            None
        }
    }
}

#[repr(C)]
#[repr(align(65536))]
pub struct L3PageTable {
    pub entries: [L3Entry; 8192],
}
const_assert_size!(L3PageTable, PAGE_SIZE);

impl L3PageTable {
    /// Returns a new `L3PageTable`.
    fn new() -> L3PageTable {
        L3PageTable {
            entries: [L3Entry::new(); 8192]
        }
    }

    /// Returns a `PhysicalAddr` of the pagetable.
    pub fn as_ptr(&self) -> PhysicalAddr {
        (self as *const L3PageTable).into()
    }
}

#[repr(C)]
#[repr(align(65536))]
pub struct PageTable {
    pub l2: L2PageTable,
    pub l3: [L3PageTable; 2],
}

impl PageTable {
    /// Returns a new `Box` containing `PageTable`.
    /// Entries in L2PageTable should be initialized properly before return.
    fn new(perm: u64) -> Box<PageTable> {
        let mut pt = Box::new(PageTable {
            l2: L2PageTable::new(),
            l3: [L3PageTable::new(), L3PageTable::new()]
        });

        for i in 0..pt.l3.len() {
            pt.l2.entries[i].set_value(pt.l3[i].as_ptr().as_u64() >> 16, RawL2Entry::ADDR);
            pt.l2.entries[i].set_value(EntryValid::Valid, RawL2Entry::VALID);
            pt.l2.entries[i].set_value(EntryType::Table, RawL2Entry::TYPE);
            pt.l2.entries[i].set_value(EntryAttr::Mem, RawL2Entry::ATTR);
            pt.l2.entries[i].set_value(perm, RawL2Entry::AP);
            pt.l2.entries[i].set_value(EntrySh::ISh, RawL2Entry::SH);
            pt.l2.entries[i].set_value(1, RawL2Entry::AF);
        }

        pt
    }

    /// Returns the (L2index, L3index) extracted from the given virtual address.
    /// Since we are only supporting 1GB virtual memory in this system, L2index
    /// should be smaller than 2.
    ///
    /// # Panics
    ///
    /// Panics if the virtual address is not properly aligned to page size.
    /// Panics if extracted L2index exceeds the number of L3PageTable.
    fn locate(va: VirtualAddr) -> (usize, usize) {
        let l2_index = (va.as_usize() >> 29) & 0x1FFF;
        let l3_index = (va.as_usize() >> 16) & 0x1FFF;


        if l2_index > 1 {
            panic!("Level 2 Index greater than number of Level 3 tables: {}", l2_index);
        }

        if va.as_usize() % PAGE_SIZE != 0 {
            panic!("virtual address not aligned {:b}", va.as_u64());
        }

        (l2_index, l3_index)

    }

    /// Returns `true` if the L3entry indicated by the given virtual address is valid.
    /// Otherwise, `false` is returned.
    pub fn is_valid(&self, va: VirtualAddr) -> bool {
        let (l2_index, l3_index) = Self::locate(va);
        self.l3[l2_index].entries[l3_index].is_valid()
    }

    /// Returns `true` if the L3entry indicated by the given virtual address is invalid.
    /// Otherwise, `true` is returned.
    pub fn is_invalid(&self, va: VirtualAddr) -> bool {
        !self.is_valid(va)
    }

    /// Set the given RawL3Entry `entry` to the L3Entry indicated by the given virtual
    /// address.
    pub fn set_entry(&mut self, va: VirtualAddr, entry: RawL3Entry) -> &mut Self {
        let (l2_index, l3_index) = Self::locate(va);
        self.l3[l2_index].entries[l3_index] = L3Entry(entry);
        self // why?
    }

    /// Returns a base address of the pagetable. The returned `PhysicalAddr` value
    /// will point the start address of the L2PageTable.
    pub fn get_baddr(&self) -> PhysicalAddr {
        self.l2.as_ptr()
    }
}

impl<'a> IntoIterator for &'a PageTable {
    type Item = &'a L3Entry;
    // this type seems wrong
    type IntoIter = core::iter::Chain<core::slice::Iter<'a, L3Entry>, core::slice::Iter<'a, L3Entry>>;

    fn into_iter(self) -> Self::IntoIter {
        self.l3[0].entries.iter().chain(self.l3[1].entries.iter())
    }
}

pub struct KernPageTable(Box<PageTable>);

impl KernPageTable {
    /// Returns a new `KernPageTable`. `KernPageTable` should have a `Pagetable`
    /// created with `KERN_RW` permission.
    ///
    /// Set L3entry of ARM physical address starting at 0x00000000 for RAM and
    /// physical address range from `IO_BASE` to `IO_BASE_END` for peripherals.
    /// Each L3 entry should have correct value for lower attributes[10:0] as well
    /// as address[47:16]. Refer to the definition of `RawL3Entry` in `vmsa.rs` for
    /// more details.
    pub fn new() -> KernPageTable {
        let mut pt = PageTable::new(EntryPerm::KERN_RW);

        // if this unwrap panics, we have massive problems
        let (_, end_addr) = allocator::memory_map().unwrap();
        let mut i = 0x0000_0000;
        // create a page table entry for every possible page in phyiscal memory 
        while i < end_addr {
            let mut entry = RawL3Entry::new(0);

            entry.set_value((i >> 16) as u64, RawL3Entry::ADDR);
            entry.set_value(EntryValid::Valid, RawL3Entry::VALID);
            entry.set_value(PageType::Page, RawL3Entry::TYPE);
            entry.set_value(EntryAttr::Mem, RawL3Entry::ATTR);
            entry.set_value(EntryPerm::KERN_RW, RawL3Entry::AP);
            entry.set_value(EntrySh::ISh, RawL3Entry::SH);
            entry.set_value(1, RawL3Entry::AF);

            pt.set_entry(i.into(), entry);

            i += PAGE_SIZE;
        }

        // create page table entries for MMIO
        // we might be able to do this in the first loop
        i = IO_BASE as usize;

        while i < IO_BASE_END {
            let mut entry = RawL3Entry::new(0);

            // mem set as dev, outer shareable (only diff between this and above)
            entry.set_value((i >> 16) as u64, RawL3Entry::ADDR);
            entry.set_value(EntryValid::Valid, RawL3Entry::VALID);
            entry.set_value(PageType::Page, RawL3Entry::TYPE);
            entry.set_value(EntryAttr::Dev, RawL3Entry::ATTR);
            entry.set_value(EntryPerm::KERN_RW, RawL3Entry::AP);
            entry.set_value(EntrySh::OSh, RawL3Entry::SH);
            entry.set_value(1, RawL3Entry::AF);

            pt.set_entry(i.into(), entry);

            i += PAGE_SIZE;
        }

        KernPageTable(pt)
    }
}

pub enum PagePerm {
    RW,
    RO,
    RWX,
}

pub struct UserPageTable(Box<PageTable>);

impl UserPageTable {
    /// Returns a new `UserPageTable` containing a `PageTable` created with
    /// `USER_RW` permission.
    pub fn new() -> UserPageTable {
        UserPageTable(PageTable::new(EntryPerm::USER_RW))
    }

    /// Allocates a page and set an L3 entry translates given virtual address to the
    /// physical address of the allocated page. Returns the allocated page.
    ///
    /// # Panics
    /// Panics if the virtual address is lower than `USER_IMG_BASE`.
    /// Panics if the virtual address has already been allocated.
    /// Panics if allocator fails to allocate a page.
    ///
    /// TODO. use Result<T> and make it failurable
    /// TODO. use perm properly
    pub fn alloc(&mut self, va: VirtualAddr, _perm: PagePerm) -> &mut [u8] {
        if va.as_usize() < USER_IMG_BASE {
            panic!("attempted to allocate memory starting at kernel space address!!");
        }

        let internal_va: VirtualAddr = (va.as_usize() - USER_IMG_BASE).into(); // translate address into this proc's space

        if self.0.is_valid(internal_va) {
            panic!("attempted to allocate a previously allocated page");
        }

        let pa = unsafe { ALLOCATOR.alloc(Page::layout()) };// allocate a physical page

        if pa == core::ptr::null_mut() {
            panic!("failed to allocate requested physical page");
        }

        let mut entry = RawL3Entry::new(0);
        entry.set_value((pa as u64) >> 16, RawL3Entry::ADDR);
        entry.set_value(EntryValid::Valid, RawL3Entry::VALID);
        entry.set_value(PageType::Page, RawL3Entry::TYPE);
        entry.set_value(EntryAttr::Mem, RawL3Entry::ATTR);
        entry.set_value(EntryPerm::USER_RW, RawL3Entry::AP);
        entry.set_value(EntrySh::ISh, RawL3Entry::SH);
        entry.set_value(1, RawL3Entry::AF);

        self.0.set_entry(internal_va, entry);

        unsafe { core::slice::from_raw_parts_mut(pa, PAGE_SIZE) }
    }
}

impl Deref for KernPageTable {
    type Target = PageTable;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for UserPageTable {
    type Target = PageTable;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KernPageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl DerefMut for UserPageTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for UserPageTable {
    fn drop(&mut self) {
        // iterate over the L3 entries and deallocate the mapped ones
        for entry in self.into_iter() {
            if let Some(pa) = entry.get_page_addr() {
                unsafe { 
                    ALLOCATOR.dealloc(pa.as_ptr() as *mut u8, Page::layout());
                }
            }
        }
    }
}

impl fmt::Debug for UserPageTable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BASE: {:x}\n", self.get_baddr().as_u64())
    }
}
