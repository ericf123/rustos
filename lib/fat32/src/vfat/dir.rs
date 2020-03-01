use alloc::string::String;
use alloc::vec::Vec;

use shim::const_assert_size;
use shim::ffi::OsStr;
use shim::io;
use shim::ioerr;
use shim::newioerr;
use core::marker::PhantomData;

use crate::traits;
use crate::util::VecExt;
use crate::vfat::{Attributes, Date, Metadata, Time, Timestamp};
use crate::vfat::{Cluster, Entry, File, VFatHandle, VFat};
use crate::traits::Dummy;

//use core::str::{from_utf8, from_utf16};

#[derive(Debug)]
pub struct Dir<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub start_cluster: Cluster,
    pub metadata: Metadata
}

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct VFatRegularDirEntry {
    pub filename: [u8; 8],
    pub extension: [u8; 3],
    pub attributes: Attributes,
    _r0: u8,
    pub created_time_tenths: u8,
    pub created_time: Time,
    pub created_date: Date, 
    pub accessed_date: Date, 
    pub cluster_high: u16, 
    pub modified_time: Time, 
    pub modified_date: Date, 
    pub cluster_low: u16,
    pub size: u32
}

const_assert_size!(VFatRegularDirEntry, 32);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct VFatLfnDirEntry {
    pub seq_num: u8,
    pub name_chars_1: [u16; 5],
    pub attributes: u8, 
    pub vfat_type: u8, 
    pub name_checksum: u8,
    pub name_chars_2: [u16; 6],
    _r0: u16, 
    pub name_chars_3: [u16; 2]
}

const_assert_size!(VFatLfnDirEntry, 32);

#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct VFatUnknownDirEntry {
    pub status: u8,
    _r0: [u8; 10],
    pub attributes: Attributes,
    _r1: [u8; 20]
}

const_assert_size!(VFatUnknownDirEntry, 32);

pub union VFatDirEntry {
    unknown: VFatUnknownDirEntry,
    regular: VFatRegularDirEntry,
    long_filename: VFatLfnDirEntry,
}

/*impl fmt::Debug for VFatDirEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), Error> {
        Ok(())
    }
}*/

impl<HANDLE: VFatHandle> Dir<HANDLE> {
    /// Finds the entry named `name` in `self` and returns it. Comparison is
    /// case-insensitive.
    ///
    /// # Errors
    ///
    /// If no entry with name `name` exists in `self`, an error of `NotFound` is
    /// returned.
    ///
    /// If `name` contains invalid UTF-8 characters, an error of `InvalidInput`
    /// is returned.
    pub fn find<P: AsRef<OsStr>>(&self, name: P) -> io::Result<Entry<HANDLE>> {
        use traits::Dir;
        for entry in self.entries()? {
            let entry_name = match &entry {
                Entry::File(f) => &f.metadata.filename,
                Entry::Dir(d) => &d.metadata.filename
            };
            let lower_name = match name.as_ref().to_str() {
                Some(s) => s,
                None => return ioerr!(InvalidInput, "name contained invalid utf8")
            };

            if str::eq_ignore_ascii_case(&entry_name, lower_name) {
                return Ok(entry);
            }
        }

        ioerr!(NotFound, "Entry not found in directory")
    }
}

impl<HANDLE: VFatHandle> traits::Dir for Dir<HANDLE> {
    /// The type of entry stored in this directory.
    type Entry = Entry<HANDLE>;

    /// An type that is an iterator over the entries in this directory.
    type Iter = DirIterator<HANDLE>;

    /// Returns an interator over the entries in this directory.
    fn entries(&self) -> io::Result<Self::Iter> {
        let mut raw_data: Vec<u8> = Vec::new();
        let bytes_per_cluster = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<u32> {
            vfat.read_chain(self.start_cluster, &mut raw_data)?;
            Ok(vfat.bytes_per_sector as u32 * vfat.sectors_per_cluster as u32)
        })?;

        Ok(DirIterator {
            data: unsafe { raw_data.cast() },
            curr_idx: 0,
            vfat: self.vfat.clone(),
            bytes_per_cluster:  bytes_per_cluster
        })
    }
}

pub struct DirIterator<HANDLE: VFatHandle>{
    data: Vec<VFatDirEntry>,
    curr_idx: usize,
    vfat: HANDLE,
    bytes_per_cluster: u32
}

impl<HANDLE: VFatHandle> Iterator for DirIterator<HANDLE> {
    type Item = Entry<HANDLE>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut lfn = [0u16; 260];
        let mut has_lfn = false;
        while self.curr_idx < self.data.len() {
            let curr_entry = &self.data[self.curr_idx];
            let as_unknown = unsafe { curr_entry.unknown };

            // check the entry's status
            match as_unknown.status {
                0x00 => return None, // end of directory
                0xE5 => { self.curr_idx += 1; continue; }, // deleted/unused                
                _ => ()
            };

            if as_unknown.attributes.0 == 0x0F {
                // LFN entry
                let lfn_entry = unsafe { curr_entry.long_filename };
                let mut name_chars = [0u16; 13];
                name_chars[..6].clone_from_slice(&{lfn_entry.name_chars_1});
                name_chars[6..11].clone_from_slice(&{lfn_entry.name_chars_2});
                name_chars[11..13].clone_from_slice(&{lfn_entry.name_chars_2});

                let mut end_name_chars = 0;

                for i in 0..name_chars.len() {
                    end_name_chars = i;
                    if name_chars[i] == 0x00 {
                        break;
                    }
                }

                /*let mut start_idx = 0;
                if lfn_entry.seq_num & (1 << 5) != 0 {
                    start_idx = lfn_entry.seq_num;
                }*/

                let start_idx = ((lfn_entry.seq_num - 1) as usize) * 13;
                // copy name chars from this entry into lfn
                lfn[start_idx..start_idx + end_name_chars].clone_from_slice(&name_chars[..end_name_chars + 1]);
                self.curr_idx += 1;
                has_lfn = true;
                continue;
            } else {
                // regular entry
                let regular_entry = unsafe { curr_entry.regular };
                let name = match has_lfn {
                    true => String::from(String::from_utf16(&lfn).unwrap().trim_end()),
                    false => String::from(core::str::from_utf8(&regular_entry.filename).unwrap().trim_end())
                };

                let metadata = Metadata {
                    filename: name,
                    extension: String::from(core::str::from_utf8(&regular_entry.extension).unwrap().trim_end()),
                    attributes: regular_entry.attributes,
                    created_timestamp: Timestamp { date: regular_entry.created_date, time: regular_entry.created_time},
                    accessed_date: Timestamp { date: regular_entry.accessed_date, time: Time::default() },
                    modified_timestamp: Timestamp { date: regular_entry.modified_date, time: regular_entry.modified_time },
                    size: regular_entry.size,
                    start_cluster: Cluster((regular_entry.cluster_high as u32) << 16 | regular_entry.cluster_low as u32)
                };

                if regular_entry.attributes.0 == 0x10 {
                    return Some(
                        Entry::Dir(Dir {
                            vfat: self.vfat.clone(),
                            start_cluster: (&metadata).start_cluster,
                            metadata: metadata
                        })
                    );
                } else {
                    return Some(
                        Entry::File(File {
                            vfat: self.vfat.clone(),
                            start_cluster: (&metadata).start_cluster.clone(),
                            name: format!("{}.{}", (&metadata).filename, (&metadata).extension),
                            metadata: metadata,
                            current_offset: 0,
                        })
                    );
                }
            }
        }

        None
    }
}
