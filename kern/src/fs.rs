pub mod sd;

use alloc::rc::Rc;
use core::fmt::{self, Debug};
use shim::io;
use shim::ioerr;
use shim::path::Path;

pub use fat32::traits;
use fat32::vfat::{Dir, Entry, File, VFat, VFatHandle};

use self::sd::Sd;
use crate::mutex::Mutex;

#[derive(Clone)]
pub struct PiVFatHandle(Rc<Mutex<VFat<Self>>>);

// These impls are *unsound*. We should use `Arc` instead of `Rc` to implement
// `Sync` and `Send` trait for `PiVFatHandle`. However, `Arc` uses atomic memory
// access, which requires MMU to be initialized on ARM architecture. Since we
// have enabled only one core of the board, these unsound impls will not cause
// any immediate harm for now. We will fix this in the future.
unsafe impl Send for PiVFatHandle {}
unsafe impl Sync for PiVFatHandle {}

impl Debug for PiVFatHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "PiVFatHandle")
    }
}

impl VFatHandle for PiVFatHandle {
    fn new(val: VFat<PiVFatHandle>) -> Self {
        PiVFatHandle(Rc::new(Mutex::new(val)))
    }

    fn lock<R>(&self, f: impl FnOnce(&mut VFat<PiVFatHandle>) -> R) -> R {
        f(&mut self.0.lock())
    }
}
pub struct FileSystem(Mutex<Option<PiVFatHandle>>);

impl FileSystem {
    /// Returns an uninitialized `FileSystem`.
    ///
    /// The file system must be initialized by calling `initialize()` before the
    /// first memory allocation. Failure to do will result in panics.
    pub const fn uninitialized() -> Self {
        FileSystem(Mutex::new(None))
    }

    /// Initializes the file system.
    /// The caller should assure that the method is invoked only once during the
    /// kernel initialization.
    ///
    /// # Panics
    ///
    /// Panics if the underlying disk or file sytem failed to initialize.
    pub unsafe fn initialize(&self) {
        let sd = Sd::new().unwrap();
        *self.0.lock() = Some(VFat::<PiVFatHandle>::from(sd).unwrap());
    }
}

impl fat32::traits::FileSystem for &FileSystem {
    type File = File<PiVFatHandle>;
    type Dir = Dir<PiVFatHandle>;
    type Entry = Entry<PiVFatHandle>;

    /// Opens the entry at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute, an error kind of `InvalidInput` is returned.
    ///
    /// If any component but the last in `path` does not refer to an existing
    /// directory, an error kind of `InvalidInput` is returned.
    ///
    /// If there is no entry at `path`, an error kind of `NotFound` is returned.
    ///
    /// All other error values are implementation defined.
    fn open<P: AsRef<Path>>(self, p: P) -> io::Result<Self::Entry> {
        if !p.as_ref().is_absolute() {
            return ioerr!(InvalidInput, "path must be absolute");
        }
        
        self.0.lock().as_ref().unwrap().open(p)
    }

    /*
    /// Opens the file at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// In addition to the error conditions for `open()`, this method returns an
    /// error kind of `Other` if the entry at `path` is not a regular file.
    fn open_file<P: AsRef<Path>>(self, path: P) -> io::Result<Self::File> {
        self.open(path)?
            .into_file()
            .ok_or(io::Error::new(io::ErrorKind::Other, "not a regular file"))
    }

    /// Opens the directory at `path`. `path` must be absolute.
    ///
    /// # Errors
    ///
    /// In addition to the error conditions for `open()`, this method returns an
    /// error kind of `Other` if the entry at `path` is not a directory.
    fn open_dir<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Dir> {
        self.open(path)?
            .into_dir()
            .ok_or(io::Error::new(io::ErrorKind::Other, "not a directory"))
    }*/
}
