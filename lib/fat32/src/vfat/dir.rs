use alloc::string::String;
use alloc::vec::Vec;

use shim::const_assert_size;
use shim::ffi::OsStr;
use shim::io;
use shim::ioerr;
use shim::newioerr;
use core::marker::PhantomData;
use core::mem::size_of;

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
    pub name: String,
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
#[derive(Copy, Clone, Debug, Default)]
pub struct VFatUnknownDirEntry {
    pub status: u8,
    _r0: [u8; 10],
    pub attributes: Attributes,
    _r1: [u8; 20]
}

const_assert_size!(VFatUnknownDirEntry, 32);

#[derive(Copy, Clone)]
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

            if str::eq_ignore_ascii_case(entry_name, lower_name) {
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

        /*let num_entries = raw_data.len() / size_of::<VFatDirEntry>();
        let mut entries = vec![
            VFatDirEntry {
                unknown: VFatUnknownDirEntry::default(),
            };
            num_entries
        ];
        unsafe {
            raw_data.as_ptr().copy_to(
                entries.as_mut_ptr() as *mut u8,
                num_entries * size_of::<VFatDirEntry>(),
            );
        }*/


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

    /*fn next(&mut self) -> Option<Self::Item> {
        let mut long_file_name = [0u16; 260];
        while self.current_index < self.data.len() {
            let current_entry: &VFatDirEntry = &self.data[self.current_index];
            let unknown_entry = unsafe { current_entry.unknown };
            if unknown_entry.status == 0x00 {
                // End of FAT
                return None;
            } else if unknown_entry.status == 0xE5 {
                // Deleted entry
                self.current_index += 1;
                continue;
            }

            // Normal entry,
            self.current_index += 1;
            if unknown_entry.attributes.lfn() {
                let lfn_entry = unsafe { current_entry.long_filename };
                let lfn_sequence_num = (lfn_entry.sequence_number & 0x1F) as usize - 1;

                if lfn_sequence_num <= 19 {
                    long_file_name[lfn_sequence_num * 13..lfn_sequence_num * 13 + 5]
                        .copy_from_slice(&lfn_entry.name_characters);
                    long_file_name[lfn_sequence_num * 13 + 5..lfn_sequence_num * 13 + 11]
                        .copy_from_slice(&lfn_entry.name_characters_2);
                    long_file_name[lfn_sequence_num * 13 + 11..lfn_sequence_num * 13 + 13]
                        .copy_from_slice(&lfn_entry.name_characters_3);
                }
            } else {
                let regular_entry = unsafe { current_entry.regular };
                let mut short_file_name = regular_entry.short_file_name;
                if short_file_name[0] == 0x05 {
                    // 0x05 is used for real 0xE5 as first byte
                    short_file_name[0] = 0xE5;
                }
                let name = str::from_utf8(&short_file_name).unwrap().trim_end();
                let ext = str::from_utf8(&regular_entry.short_file_extension)
                    .unwrap()
                    .trim_end();
                let mut short_name = String::from(name);
                if !ext.is_empty() {
                    short_name.push_str(".");
                    short_name.push_str(ext);
                }
                let mut nul_byte_index = None;
                for (i, byte) in long_file_name.iter().enumerate() {
                    if *byte == 0 {
                        nul_byte_index = Some(i);
                        break;
                    }
                }
                let long_name = String::from_utf16(if let Some(len) = nul_byte_index {
                    &long_file_name[0..len]
                } else {
                    &long_file_name
                })
                .unwrap();
                if regular_entry.metadata.attributes.directory() {
                    return Some(Entry::Dir(Dir {
                        cluster: Cluster::from(regular_entry.metadata.first_cluster()),
                        fs: self.fs.clone(),
                        short_name,
                        long_name,
                        metadata: regular_entry.metadata,
                    }));
                } else {
                    return Some(Entry::File(File {
                        cluster: Cluster::from(regular_entry.metadata.first_cluster()),
                        fs: self.fs.clone(),
                        short_name,
                        long_name,
                        metadata: regular_entry.metadata,
                        file_size: regular_entry.file_size,
                        current_offset: 0,
                        current_cluster: Some(Cluster::from(
                            regular_entry.metadata.first_cluster(),
                        )),
                        bytes_per_cluster: self.bytes_per_cluster,
                    }));
                }
            }
        }
        None
    }*/

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

            self.curr_idx += 1;
            if as_unknown.attributes.0 == 0x0F {
                // LFN entry
                let lfn_entry = unsafe { curr_entry.long_filename };
                let mut name_chars = [0u16; 13];
                name_chars[..5].clone_from_slice(&{lfn_entry.name_chars_1});
                name_chars[5..11].clone_from_slice(&{lfn_entry.name_chars_2});
                name_chars[11..13].clone_from_slice(&{lfn_entry.name_chars_3});

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
                //println!("{}", String::from_utf16(&name_chars).unwrap());
                let start_idx = (((lfn_entry.seq_num & 0x1F) - 1) as usize) * 13;
                // copy name chars from this entry into lfn
                lfn[start_idx..=start_idx + end_name_chars].clone_from_slice(&name_chars[..=end_name_chars]);
                has_lfn = true;
                continue;
            } else {
                // regular entry
                let regular_entry = unsafe { curr_entry.regular };
                //println!("{:#?}", regular_entry);
                let name = match has_lfn {
                    true => {
                        //let real_lfn = lfn.iter().take_while(|c| **c != 0).as_slice();
                        let mut last = 0; 
                        for i in 0..lfn.len() {
                            if lfn[i] != 0 {
                                last += 1;
                            } else {
                                break;
                            }
                        }
                        String::from(String::from_utf16(&lfn[..last]).unwrap().trim_end())
                    },
                    false => { 
                        let filename = core::str::from_utf8(&regular_entry.filename).unwrap().trim_end();
                        let extension =  core::str::from_utf8(&regular_entry.extension).unwrap().trim_end();
                        let mut final_;
                        if extension.len() > 0 {
                            final_ = format!("{}.{}", filename, extension)
                        } else {
                            final_ = String::from(filename).trim_end().to_string();
                        }

                        final_
                    }
                };


                /*let name = match has_lfn {
                    true => String::from(String::from_utf16(&lfn).unwrap().trim_end()),
                    false => unsafe { String::from(core::str::from_utf8_unchecked(&regular_entry.filename).trim_end()) }
                };*/

                let metadata = Metadata {
                    filename: name.clone(),
                    extension: String::from(core::str::from_utf8(&regular_entry.extension).unwrap().trim_end()),
                    attributes: regular_entry.attributes,
                    created_timestamp: Timestamp { date: regular_entry.created_date, time: regular_entry.created_time},
                    accessed_date: Timestamp { date: regular_entry.accessed_date, time: Time::default() },
                    modified_timestamp: Timestamp { date: regular_entry.modified_date, time: regular_entry.modified_time },
                    size: regular_entry.size,
                    start_cluster: Cluster((regular_entry.cluster_high as u32) << 16 | regular_entry.cluster_low as u32)
                };
                //println!("{}", name);
                if (regular_entry.attributes.0 & 0x10) != 0 {
                    return Some(
                        Entry::Dir(Dir {
                            vfat: self.vfat.clone(),
                            start_cluster: (&metadata).start_cluster,
                            metadata: metadata,
                            name: name
                        })
                    );
                } else {
                    return Some(
                        Entry::File(File {
                            vfat: self.vfat.clone(),
                            start_cluster: (&metadata).start_cluster.clone(),
                            current_cluster: Some((&metadata).start_cluster.clone()),
                            name: name,
                            metadata: metadata,
                            current_offset: 0,
                            size: regular_entry.size,
                            bytes_per_cluster: self.bytes_per_cluster
                        })
                    );
                }
            }
        }

        None
    }
}
