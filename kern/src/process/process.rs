use alloc::boxed::Box;
use shim::io;
use shim::path::Path;
use crate::param::*;
use crate::process::{Stack, State};
use crate::traps::TrapFrame;
use crate::vm::*;
use kernel_api::{OsError, OsResult};
use core::mem;
use crate::FILESYSTEM;
use fat32::traits::FileSystem;
use crate::fs::PiVFatHandle;
use fat32::vfat::File;
use core::cmp::min;
use crate::allocator::util::align_down;

/// Type alias for the type of a process ID.
pub type Id = u64;

/// A structure that represents the complete state of a process.
#[derive(Debug)]
pub struct Process {
    /// The saved trap frame of a process.
    pub context: Box<TrapFrame>,
    /// The memory allocation used for the process's stack.
    pub stack: Stack,
    /// The page table describing the Virtual Memory of the process
    pub vmap: Box<UserPageTable>,
    /// The scheduling state of the process.
    pub state: State,
}

impl Process {
    /// Creates a new process with a zeroed `TrapFrame` (the default), a zeroed
    /// stack of the default size, and a state of `Ready`.
    ///
    /// If enough memory could not be allocated to start the process, returns
    /// `None`. Otherwise returns `Some` of the new `Process`.
    pub fn new() -> OsResult<Process> {
        let stack = match Stack::new() {
            Some(s) => s,
            None => return Err(OsError::NoMemory)
        };

        Ok(
            Process {
                context: Box::new(TrapFrame::default()),
                stack,
                vmap: Box::new(UserPageTable::new()),
                state: State::Ready
            }
        )
    }

    /// Load a program stored in the given path by calling `do_load()` method.
    /// Set trapframe `context` corresponding to the its page table.
    /// `sp` - the address of stack top
    /// `elr` - the address of image base.
    /// `ttbr0` - the base address of kernel page table
    /// `ttbr1` - the base address of user page table
    /// `spsr` - `F`, `A`, `D` bit should be set.
    ///
    /// Returns Os Error if do_load fails.
    pub fn load<P: AsRef<Path>>(pn: P) -> OsResult<Process> {
        use crate::VMM;

        let mut p = Process::do_load(pn)?;

        // set up context
        p.context.ttbr0 = VMM.get_baddr().as_u64();
        p.context.ttbr1 = p.vmap.get_baddr().as_u64();
        p.context.elr = Self::get_image_base().as_u64();
        p.context.sp = Self::get_stack_top().as_u64();
        // set bit 4 to be in aarch64 (0)
        // set bits 0-3 to execute in EL0, correct sp (0)
        // unmask irq interrupts bit 7 = 0
        p.context.spsr = 0b1101_00_0000;

        Ok(p)
    }

    /// Creates a process and open a file with given path.
    /// Allocates one page for stack with read/write permission, and N pages with read/write/execute
    /// permission to load file's contents.
    fn do_load<P: AsRef<Path>>(pn: P) -> OsResult<Process> {
        use io::Read;
        // create a process struct
        let mut loaded_proc = Process::new()?;
        loaded_proc.vmap.alloc(Self::get_stack_base(), PagePerm::RW); // allocate a page for the stack

        // open the file
        let mut bin_file: File<PiVFatHandle> = match FILESYSTEM.open_file(pn) {
            Ok(f) => f,
            Err(_) => return Err(OsError::IoError)
        };

        // allocate file pages, read into them
        let bin_size = bin_file.size as usize;
        let num_pages = (bin_size / PAGE_SIZE) + 1;

        let mut remaining_bytes = bin_size;
        for page_num in 0..num_pages {
            let page_va = Self::get_image_base() + (PAGE_SIZE * (page_num as usize)).into();
            let page = loaded_proc.vmap.alloc(page_va.into(), PagePerm::RWX); // allocate a page 
            let bytes_to_read = min(PAGE_SIZE, remaining_bytes);

            if let Err(_) = bin_file.read_exact(&mut page[0..bytes_to_read]) {
                return Err(OsError::IoError);
            }
            remaining_bytes -= bytes_to_read;
        }

        Ok(loaded_proc)
    }

    /// Returns the highest `VirtualAddr` that is supported by this system.
    pub fn get_max_va() -> VirtualAddr {
        (USER_IMG_BASE + USER_MAX_VM_SIZE).into()
    }

    /// Returns the `VirtualAddr` represents the base address of the user
    /// memory space.
    pub fn get_image_base() -> VirtualAddr {
        USER_IMG_BASE.into()
    }

    /// Returns the `VirtualAddr` represents the base address of the user
    /// process's stack.
    pub fn get_stack_base() -> VirtualAddr {
        USER_STACK_BASE.into()
    }

    /// Returns the `VirtualAddr` represents the top of the user process's
    /// stack.
    pub fn get_stack_top() -> VirtualAddr {
        //(USER_STACK_BASE.wrapping_add(PAGE_SIZE)).into()
        align_down(usize::max_value(), 16).into()
    }

    /// Returns `true` if this process is ready to be scheduled.
    ///
    /// This functions returns `true` only if one of the following holds:
    ///
    ///   * The state is currently `Ready`.
    ///
    ///   * An event being waited for has arrived.
    ///
    ///     If the process is currently waiting, the corresponding event
    ///     function is polled to determine if the event being waiting for has
    ///     occured. If it has, the state is switched to `Ready` and this
    ///     function returns `true`.
    ///
    /// Returns `false` in all other cases.
    pub fn is_ready(&mut self) -> bool {
        let state = mem::replace(&mut self.state, State::Ready);

        if let State::Ready = state {
            return true;
        } else if let State::Waiting(mut done) = state {
            if done(self) {
                return true;
            } else {
                mem::replace(&mut self.state, State::Waiting(done));
            }
        }

        return false;
    }
}
