use core::alloc::Layout;
use core::fmt;
use core::ptr;

use crate::allocator::linked_list::LinkedList;
use crate::allocator::util::*;
use crate::allocator::LocalAlloc;

/// A simple allocator that allocates based on size classes.
///   bin 0 (2^3 bytes)    : handles allocations in (0, 2^3]
///   bin 1 (2^4 bytes)    : handles allocations in (2^3, 2^4]
///   ...
///   bin 29 (2^22 bytes): handles allocations in (2^31, 2^32]
///   
///   map_to_bin(size) -> k
///   

pub struct Allocator {
    bins: [LinkedList; 30],
    start: usize,
    end: usize,
    free_pool_start: usize
}

struct FreeHeader {
    // this space gets used by the linked list
    _prev: usize, 
    // size of the free region in bytes
    size: usize
}

impl Allocator {
    /// Creates a new bin allocator that will allocate memory from the region
    /// starting at address `start` and ending at address `end`.
    pub fn new(start: usize, end: usize) -> Allocator {
        let bins = [LinkedList::new(); 30];
        Allocator {
            bins,
            start,
            end,
            free_pool_start: start
        }
    }

    fn map_to_bin(&self, size: usize) -> usize {
        let mut k = 3;
        let mut div = 8;
        
        // iterate over powers of 2 until size < 2^k
        while size / div > 0 {
            k += 1;
            div = div << 2;
        }

        k - 3
    }


    // returns a pointer to memory area of requested size
    fn alloc_for_size(size: usize) -> *mut u8 {
        unimplemented!(); 
    }

    fn debug_bins(&self) {
        println!("-----bins-----");
        for i in 0..self.bins.len() {
            print!("{}: ", i);
            for node in self.bins[i].iter() {
                print!("{:#x} -> ", node as usize);
            }
            println!();
        }
    }
}

impl LocalAlloc for Allocator {
    /// Allocates memory. Returns a pointer meeting the size and alignment
    /// properties of `layout.size()` and `layout.align()`.
    ///
    /// If this method returns an `Ok(addr)`, `addr` will be non-null address
    /// pointing to a block of storage suitable for holding an instance of
    /// `layout`. In particular, the block will be at least `layout.size()`
    /// bytes large and will be aligned to `layout.align()`. The returned block
    /// of storage may or may not have its contents initialized or zeroed.
    ///
    /// # Safety
    ///
    /// The _caller_ must ensure that `layout.size() > 0` and that
    /// `layout.align()` is a power of two. Parameters not meeting these
    /// conditions may result in undefined behavior.
    ///
    /// # Errors
    ///
    /// Returning null pointer (`core::ptr::null_mut`)
    /// indicates that either memory is exhausted
    /// or `layout` does not meet this allocator's
    /// size or alignment constraints.
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut ret: Option<*mut u8> = None;
        let mut not_usable: LinkedList = LinkedList::new();
        let mut k = 0;
        while ret == None {
            if k == 0 {
                k = self.map_to_bin(layout.size());
            }
            
            // attempt to get a memory region that was previously freed
            // if we can't fit in a previously freed region, we pull
            // from the globally free region
            ret = match self.bins[k].pop() {
                Some(addr) => {
                    // we can fit in a previously freed block
                    let header = &*(addr as *mut FreeHeader);
                    let aligned = align_up(addr as usize, layout.align());

                    // make sure we can still fit after alignment
                    if aligned + layout.size() <= addr as usize + header.size {
                        Some(aligned as *mut u8)
                    } else {
                        // if we can't fit after alignment, keep looking
                        // also save the unused free block so we can put it 
                        // back in the free list later
                        not_usable.push(addr);
                        None
                    }
                }
                None => {
                    if k < self.bins.len() - 1 {
                        // we do this so we search through all the bins in the free
                        // list
                        k += 1;
                        None
                    } else {
                        // we have no previously freed blocks that could work,
                        // allocate a completely new memory region from the 
                        // global free pool
                        let aligned = align_up(self.free_pool_start, layout.align());
                        if aligned + layout.size() <= self.end {
                            self.free_pool_start = aligned + layout.size();
                            Some(aligned as *mut u8)
                        } else {
                            Some(core::ptr::null_mut())
                        }
                    }
                }
            }
        } 

        // add back all the blocks we tried to use but couldn't 
        // because of alignment problems
        for addr in not_usable.iter() {
            let header = &*(addr as *const FreeHeader);
            self.bins[self.map_to_bin(header.size)].push(addr);
        }
        
        ret.unwrap()
    }

    /// Deallocates the memory referenced by `ptr`.
    ///
    /// # Safety
    ///
    /// The _caller_ must ensure the following:
    ///
    ///   * `ptr` must denote a block of memory currently allocated via this
    ///     allocator
    ///   * `layout` must properly represent the original layout used in the
    ///     allocation call that returned `ptr`
    ///
    /// Parameters not meeting these conditions may result in undefined
    /// behavior.
    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let header_ptr = ptr as *mut FreeHeader;
        // we store 0 in _prev as a dummy, it will get overwritten 
        // when we push the item to the free list
        *header_ptr = FreeHeader { _prev: 0, size: layout.size() };
        self.bins[self.map_to_bin(layout.size())].push(ptr as *mut usize);
    }
}

impl fmt::Debug for Allocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "start: {}, end: {}, global free start: {}", self.start, self.end, self.free_pool_start)
    }
}
