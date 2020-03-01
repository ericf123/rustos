use crate::traits;
use crate::vfat::{Dir, File, Metadata, VFatHandle};
use core::fmt;

// You can change this definition if you want
#[derive(Debug)]
pub enum Entry<HANDLE: VFatHandle> {
    File(File<HANDLE>),
    Dir(Dir<HANDLE>),
}

// TODO: Implement any useful helper methods on `Entry`.

impl<HANDLE: VFatHandle> traits::Entry for Entry<HANDLE> {
    type File = File<HANDLE>;
    type Dir = Dir<HANDLE>;
    type Metadata = Metadata;

    fn name(&self) -> &str {
        &self.metadata().filename 
    }

    fn metadata(&self) -> &Self::Metadata {
        self.metadata()
    }

    fn as_file(&self) -> Option<&File<HANDLE>> {
        match &self {
            Entry::File(f) => Some(&f),
            _ => None
        }
    }

    fn as_dir(&self) -> Option<&Dir<HANDLE>> {
        match &self {
            Entry::Dir(d) => Some(&d),
            _ => None
        }
    }

    fn into_file(self) -> Option<File<HANDLE>> {
        match self {
            Entry::File(f) => Some(f),
            _ => None
        }
    }

    fn into_dir(self) -> Option<Dir<HANDLE>> {
        match self {
            Entry::Dir(d) => Some(d),
            _ => None
        }
    }
}