use alloc::boxed::Box;
use alloc::collections::vec_deque::VecDeque;
use core::fmt;
use crate::mutex::Mutex;
use crate::param::{TICK};
use crate::process::{Id, Process, State};
use crate::traps::TrapFrame;
use crate::IRQ;
use crate::SCHEDULER;
extern crate pi;
use pi::interrupt;
use pi::timer;
use core::fmt::Formatter;

/// Process scheduler for the entire machine.
#[derive(Debug)]
pub struct GlobalScheduler(Mutex<Option<Scheduler>>);

fn timer_handler(tf: &mut TrapFrame) {
    //kprintln!("timer interrupt...scheduling next one");
    timer::tick_in(TICK); 
    SCHEDULER.switch(State::Ready, tf);
}

impl GlobalScheduler {
    /// Returns an uninitialized wrapper around a local scheduler.
    pub const fn uninitialized() -> GlobalScheduler {
        GlobalScheduler(Mutex::new(None))
    }

    /// Enter a critical region and execute the provided closure with the
    /// internal scheduler.
    pub fn critical<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Scheduler) -> R,
    {
        let mut guard = self.0.lock();
        f(guard.as_mut().expect("scheduler uninitialized"))
    }


    /// Adds a process to the scheduler's queue and returns that process's ID.
    /// For more details, see the documentation on `Scheduler::add()`.
    pub fn add(&self, process: Process) -> Option<Id> {
        self.critical(move |scheduler| scheduler.add(process))
    }

    /// Performs a context switch using `tf` by setting the state of the current
    /// process to `new_state`, saving `tf` into the current process, and
    /// restoring the next process's trap frame into `tf`. For more details, see
    /// the documentation on `Scheduler::schedule_out()` and `Scheduler::switch_to()`.
    pub fn switch(&self, new_state: State, tf: &mut TrapFrame) -> Id {
        self.critical(|scheduler| scheduler.schedule_out(new_state, tf));
        self.switch_to(tf)
    }

    pub fn switch_to(&self, tf: &mut TrapFrame) -> Id {
        loop {
            let rtn = self.critical(|scheduler| scheduler.switch_to(tf));
            if let Some(id) = rtn {
                return id;
            }
            aarch64::wfi();
        }
    }

    /// Kills currently running process and returns that process's ID.
    /// For more details, see the documentaion on `Scheduler::kill()`.
    #[must_use]
    pub fn kill(&self, tf: &mut TrapFrame) -> Option<Id> {
        self.critical(|scheduler| scheduler.kill(tf))
    }

    /// Starts executing processes in user space using timer interrupt based
    /// preemptive scheduling. This method should not return under normal conditions.
    pub fn start(&self) -> ! {
        // register timer interrupt handler
        IRQ.register(interrupt::Interrupt::Timer1, Box::new(timer_handler));

        // enable timer interrupts
        let mut int_controller = interrupt::Controller::new();
        int_controller.enable(interrupt::Interrupt::Timer1);

        // set timer interrupt to occur TICK duration from now
        timer::tick_in(TICK);

        let mut bootstrap_frame = TrapFrame::default();
        self.switch_to(&mut bootstrap_frame);
        let bootstrap_frame_addr = &bootstrap_frame as *const TrapFrame as u64;
        unsafe {
            asm!("mov SP, $0
                  bl context_restore
                  adr lr, _next_page
                  mov SP, lr
                  mov lr, xzr
                  eret
                  _next_page:
                    .balign 0x10000"
                :: "r"(bootstrap_frame_addr)
                :: "volatile");
        }
        
        loop {} // satisfy the compiler
    }

    /// Initializes the scheduler and add userspace processes to the Scheduler
    pub unsafe fn initialize(&self) {
        *self.0.lock() = Some(Scheduler::new()); 
        
        self.add(Process::load("/fib").unwrap());
        self.add(Process::load("/fib").unwrap());
        self.add(Process::load("/fib").unwrap());
        self.add(Process::load("/fib").unwrap());
        self.add(Process::load("/syscall_test").unwrap());
        self.add(Process::load("/syscall_test").unwrap());
        self.add(Process::load("/syscall_test").unwrap());
        self.add(Process::load("/syscall_test").unwrap());
    }

    // The following method may be useful for testing Phase 3:
    //
    // * A method to load a extern function to the user process's page table.
    //
    /*pub fn test_phase_3(&self, proc: &mut Process){
        use crate::vm::{VirtualAddr, PagePerm};
    
        let mut page = proc.vmap.alloc(
            VirtualAddr::from(USER_IMG_BASE as u64), PagePerm::RWX);
    
        let text = unsafe {
            core::slice::from_raw_parts(test_user_process as *const u8, 24)
        };
    
        page[0..24].copy_from_slice(text);
    }*/
}

#[derive(Debug)]
pub struct Scheduler {
    processes: VecDeque<Process>,
    last_id: Option<Id>,
}

impl Scheduler {
    /// Returns a new `Scheduler` with an empty queue.
    fn new() -> Scheduler {
        Scheduler {
            processes: VecDeque::new(),
            last_id: None
        }
    }

    /// Adds a process to the scheduler's queue and returns that process's ID if
    /// a new process can be scheduled. The process ID is newly allocated for
    /// the process and saved in its `trap_frame`. If no further processes can
    /// be scheduled, returns `None`.
    ///
    /// It is the caller's responsibility to ensure that the first time `switch`
    /// is called, that process is executing on the CPU.
    fn add(&mut self, mut process: Process) -> Option<Id> {
        if self.last_id == Some(u64::max_value()) {
            return None;
        }

        let next_id = match self.last_id {
            Some(id) => id + 1,
            None => 0
        };

        process.context.tpidr = next_id;
        self.processes.push_back(process);
        self.last_id = Some(next_id);
        Some(next_id)
    }

    /// Finds the currently running process, sets the current process's state
    /// to `new_state`, prepares the context switch on `tf` by saving `tf`
    /// into the current process, and push the current process back to the
    /// end of `processes` queue.
    ///
    /// If the `processes` queue is empty or there is no current process,
    /// returns `false`. Otherwise, returns `true`.
    fn schedule_out(&mut self, new_state: State, tf: &mut TrapFrame) -> bool {
        // currently running process should be at top of queue
        if let Some(mut running_proc) = self.processes.pop_front() {
            if let State::Running = running_proc.state {
                running_proc.state = new_state;
                running_proc.context = Box::new(*tf);
                self.processes.push_back(running_proc); // pop current from front and push to back
                return true;
            }
        }

        return false;
    }

    /// Finds the next process to switch to, brings the next process to the
    /// front of the `processes` queue, changes the next process's state to
    /// `Running`, and performs context switch by restoring the next process`s
    /// trap frame into `tf`.
    ///
    /// If there is no process to switch to, returns `None`. Otherwise, returns
    /// `Some` of the next process`s process ID.
    fn switch_to(&mut self, tf: &mut TrapFrame) -> Option<Id> {
        let next_idx = self.processes.iter_mut().position(|item: &mut Process| -> bool {
            item.is_ready()
        })?;

        let mut next = self.processes.remove(next_idx)?; // returns none if index out of bounds
        *tf = *next.context; // restore context
        next.state = State::Running;
        let next_id = next.context.tpidr;
        self.processes.push_front(next); // push the running proc to the front of queue
        Some(next_id)
    }

    /// Kills currently running process by scheduling out the current process
    /// as `Dead` state. Removes the dead process from the queue, drop the
    /// dead process's instance, and returns the dead process's process ID.
    fn kill(&mut self, tf: &mut TrapFrame) -> Option<Id> {
        // stop current proc and set state to dead
        if self.schedule_out(State::Dead, tf) {
            // dead boi will be at back of queue after schedule out
            // this method needs to be syncronized for this to work properly
            let kill_me = self.processes.pop_back()?;
            let pid = kill_me.context.tpidr;
            core::mem::drop(kill_me);
            if let None = self.switch_to(tf) {
                aarch64::wfi();
            }
            Some(pid)
        } else {
            None
        }
    }
}

impl fmt::Display for Scheduler {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for process in self.processes.iter() {
            write!(f, "{}:{:#?} -> ", process.context.tpidr, process.state)?;
        }
        write!(f, "end")
    }
}

/*pub extern "C" fn  test_user_process() -> ! {
    loop {
        let ms = 10000;
        let error: u64;
        let elapsed_ms: u64;

        unsafe {
            asm!("mov x0, $2
              svc 1
              mov $0, x0
              mov $1, x7"
                 : "=r"(elapsed_ms), "=r"(error)
                 : "r"(ms)
                 : "x0", "x7"
                 : "volatile");
        }
    }
}*/

