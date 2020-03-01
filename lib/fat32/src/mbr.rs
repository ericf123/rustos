use core::fmt;
use shim::const_assert_size;
use shim::io;
use core::mem;

use crate::traits::BlockDevice;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CHS {
    head: u8,
    // bits 0-5 are sector, 6-15 are cylinder
    cs_lower_byte: u8,  
    cs_upper_byte: u8
}

impl fmt::Debug for CHS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cylinder = self.cs_lower_byte & 0x1F;
        let sector = (((self.cs_lower_byte & 0xE0) as u16) << 8) | self.cs_upper_byte as u16;
        write!(f, "({:?}, {:?}, {:?})", cylinder, self.head, sector)
    }
}

const_assert_size!(CHS, 3);

#[repr(C, packed)]
pub struct PartitionEntry {
    pub boot_indicator: u8, // 0x80 for bootable
    _start_chs: CHS,
    pub partition_type: u8,
    _end_chs: CHS,
    pub relative_sector: u32,
    pub total_sectors: u32
}

impl fmt::Debug for PartitionEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let is_bootable = self.boot_indicator == 0x80;
        write!(f, "BOOTABLE:        {:?} ({:?})\n\
                   START CHS:       {:?}\n\
                   TYPE:            {:?}\n\
                   END CHS:         {:?}\n\
                   RELATIVE SECTOR: {:?}\n\
                   TOTAL SECTORS:   {:?}",
                &{self.boot_indicator}, is_bootable, &{self._start_chs}, &{self.partition_type},
                &{self._end_chs}, &{self.relative_sector}, &{self.total_sectors})
    }
}

const_assert_size!(PartitionEntry, 16);

/// The master boot record (MBR).
#[repr(C, packed)]
pub struct MasterBootRecord {
    pub bootstrap: [u8; 436],
    pub disk_id: [u8; 10],
    pub partition_table: [PartitionEntry; 4],
    pub signature: u16
}

impl fmt::Debug for MasterBootRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DISK ID:   {:?}\n\
                   -----PARTITION 1-----\n\
                   {:?}\n\
                   -----PARTITION 2-----\n\
                   {:?}\n\
                   -----PARTITION 3-----\n\
                   {:?}\n\
                   -----PARTITION 4-----\n\
                   {:?}\n\
                   SIGNATURE: {:#x}", 
                &{self.disk_id}, &{&self.partition_table[0]}, &{&self.partition_table[1]},
                &{&self.partition_table[2]}, &{&self.partition_table[3]}, &{self.signature})
    }
}

const_assert_size!(MasterBootRecord, 512);

#[derive(Debug)]
pub enum Error {
    /// There was an I/O error while reading the MBR.
    Io(io::Error),
    /// Partiion `.0` (0-indexed) contains an invalid or unknown boot indicator.
    UnknownBootIndicator(u8),
    /// The MBR magic signature was invalid.
    BadSignature,
}

impl MasterBootRecord {
    /// Reads and returns the master boot record (MBR) from `device`.
    ///
    /// # Errors
    ///
    /// Returns `BadSignature` if the MBR contains an invalid magic signature.
    /// Returns `UnknownBootIndicator(n)` if partition `n` contains an invalid
    /// boot indicator. Returns `Io(err)` if the I/O error `err` occured while
    /// reading the MBR.
    pub fn from<T: BlockDevice>(mut device: T) -> Result<MasterBootRecord, Error> {
        // read the MBR from sector 0 of the block device, 
        // check for IO errors
        let mut sector_0_buf = [0u8; 512];
        match device.read_sector(0, &mut sector_0_buf) {
             Err(err) => return Err(Error::Io(err)),
            _ => ()
        };

        // transmute the buffer into an MBR
        let mbr = unsafe { mem::transmute::<[u8; 512], MasterBootRecord>(sector_0_buf) };

        // check the signature
        if mbr.signature != 0xAA55 {
            return Err(Error::BadSignature);
        }

        // validate bootable field of partition entries
        for i in 0..mbr.partition_table.len() {
            if !(mbr.partition_table[i].boot_indicator == 0x00
                 || mbr.partition_table[i].boot_indicator == 0x80) {
                return Err(Error::UnknownBootIndicator(i as u8));
            }
        }
        
        Ok(mbr)
    }
}
