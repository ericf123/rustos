use crate::vfat::*;
use core::fmt;

use self::Status::*;

#[derive(Debug, PartialEq)]
pub enum Status {
    /// The FAT entry corresponds to an unused (free) cluster.
    Free,
    /// The FAT entry/cluster is reserved.
    Reserved,
    /// The FAT entry corresponds to a valid data cluster. The next cluster in
    /// the chain is `Cluster`.
    Data(Cluster),
    /// The FAT entry corresponds to a bad (disk failed) cluster.
    Bad,
    /// The FAT entry corresponds to a valid data cluster. The corresponding
    /// cluster is the last in its chain.
    Eoc(u32),
}

#[derive(Clone)]
#[repr(C, packed)]
pub struct FatEntry(pub u32);

impl FatEntry {
    /// Returns the `Status` of the FAT entry `self`.
    pub fn status(&self) -> Status {
        // we need to zero the most significant four bits first
        match self.0 & !(0xF << 28) {
            0 => Status::Free,
            1 => Status::Reserved,
            next @ 2..=0x0FFFFFEF => Status::Data(Cluster::from(next)),
            0x0FFFFFF0..=0x0FFFFFF6 => Status::Reserved,
            0x0FFFFFF7 => Status::Bad,
            marker @ 0x0FFFFFF8..=0x0FFFFFFF => Status::Eoc(marker),
            _ => Status::Bad // this should definitely not happen b/c we force the first 4 bits to 0
        }
    }
}

impl fmt::Debug for FatEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FatEntry")
            .field("value", &{ self.0 })
            .field("status", &self.status())
            .finish()
    }
}
