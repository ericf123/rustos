use alloc::boxed::Box;
use pi::interrupt::Interrupt;

use crate::mutex::Mutex;
use crate::traps::TrapFrame;

pub type IrqHandler = Box<dyn FnMut(&mut TrapFrame) + Send>;
pub type IrqHandlers = [Option<IrqHandler>; Interrupt::MAX];

pub struct Irq(Mutex<Option<IrqHandlers>>);

impl Irq {
    pub const fn uninitialized() -> Irq {
        Irq(Mutex::new(None))
    }

    pub fn initialize(&self) {
        *self.0.lock() = Some([None, None, None, None, None, None, None, None]);
    }

    /// Register an irq handler for an interrupt.
    /// The caller should assure that `initialize()` has been called before calling this function.
    pub fn register(&self, int: Interrupt, handler: IrqHandler) {
        self.0.lock().as_mut().expect("register handlers")[Interrupt::to_index(int)] = Some(handler);
    }

    /// Executes an irq handler for the givven interrupt.
    /// The caller should assure that `initialize()` has been called before calling this function.
    pub fn invoke(&self, int: Interrupt, tf: &mut TrapFrame) {
        // this syntax is physically painful
        self.0.lock()
              .as_mut()
              .expect("invoke handlers")[Interrupt::to_index(int)]
              .as_mut()
              .expect("hanlder exists")(tf);
    }

    pub fn handler_exists(&self, int: Interrupt) -> bool {
        match &self.0.lock().as_mut().expect("irq not initialized")[Interrupt::to_index(int)] {
            Some(_) => true,
            None => false
        }
    }

}
