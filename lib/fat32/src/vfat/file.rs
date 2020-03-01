use alloc::string::String;

use shim::io::{self, SeekFrom};
use shim::ioerr;

use crate::traits;
use crate::vfat::{Cluster, Metadata, VFatHandle, VFat, Status, FatEntry};
use crate::vfat;

use core::cmp::min;

#[derive(Debug)]
pub struct File<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub metadata: Metadata,
    pub current_offset: u32,
    pub start_cluster: Cluster,
    pub name: String,
    pub size: u32,
    pub current_cluster: Option<Cluster>,
    pub bytes_per_cluster: u32
}

// FIXME: Implement `traits::File` (and its supertraits) for `File`.
impl<HANDLE: VFatHandle> traits::File for File<HANDLE> {
    /// Writes any buffered data to disk.
    fn sync(&mut self) -> io::Result<()> {
        unimplemented!()
    }

    /// Returns the size of the file in bytes.
    fn size(&self) -> u64 {
        self.size as u64
    }
}

impl<HANDLE: VFatHandle> io::Write for File<HANDLE>  {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        panic!("Dummy")
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("Dummy")
    }
}

impl<HANDLE:VFatHandle> io::Read for File<HANDLE> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.size() == 0 {
            return Ok(0);
        }
        use traits::File;
        use io::Seek;

        let read_size = min(buf.len(), (self.size() - self.current_offset as u64) as usize);
        let mut cluster_offset = (self.current_offset % self.bytes_per_cluster) as usize;
        let mut remaining = read_size;
        let mut current_cluster = self.current_cluster;
        let mut buf_offset = 0;

        while remaining > 0 {
            let cluster_read_size = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<usize> {
                vfat.read_cluster(current_cluster.unwrap(), cluster_offset, &mut buf[buf_offset..read_size])
            })?;

            if cluster_read_size == self.bytes_per_cluster as usize - cluster_offset {
                let entry = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<FatEntry> {
                    Ok(((*vfat.fat_entry(current_cluster.unwrap())?).clone()))
                })?;

                match entry.status() {
                    Status::Eoc(_) => current_cluster = None,
                    Status::Data(next) => {
                        current_cluster = Some(next);
                    },
                    _ => return ioerr!(InvalidData, "invalid cluster chain")

                }
            }
            buf_offset += cluster_read_size;
            remaining -= cluster_read_size;
            cluster_offset = 0;
        }

        self.current_offset += read_size as u32;
        self.current_cluster = current_cluster;

        Ok(read_size)
    }
}

impl<HANDLE: VFatHandle> io::Seek for File<HANDLE> {
    /// Seek to offset `pos` in the file.
    ///
    /// A seek to the end of the file is allowed. A seek _beyond_ the end of the
    /// file returns an `InvalidInput` error.
    ///
    /// If the seek operation completes successfully, this method returns the
    /// new position from the start of the stream. That position can be used
    /// later with SeekFrom::Start.
    ///
    /// # Errors
    ///
    /// Seeking before the start of a file or beyond the end of the file results
    /// in an `InvalidInput` error.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        use traits::File;
        let new_offset = match pos {
            SeekFrom::Start(off) => off as i64,
            SeekFrom::Current(off) => self.current_offset as i64 + off,
            SeekFrom::End(off) => self.size() as i64 + off
        };
        
        if new_offset as u64 > self.size() || new_offset < 0 {
            return ioerr!(InvalidInput, "invalid seek");
        } else {
            let mut curr_cluster = self.start_cluster;
            for i in 0..(new_offset as u32 / self.bytes_per_cluster) {
                let entry = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<FatEntry> {
                    Ok(((*vfat.fat_entry(curr_cluster)?).clone()))
                })?;

                match entry.status() {
                    Status::Data(next) => curr_cluster = next,
                    _ => ()
                };
            }

            self.current_cluster = Some(curr_cluster);
            self.current_offset = new_offset as u32;
            return Ok(self.current_offset as u64);
        }
    }
}
