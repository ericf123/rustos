use alloc::string::String;

use shim::io::{self, SeekFrom};
use shim::ioerr;

use crate::traits;
use crate::vfat::{Cluster, Metadata, VFatHandle, VFat};
use crate::vfat;

use core::cmp::min;

#[derive(Debug)]
pub struct File<HANDLE: VFatHandle> {
    pub vfat: HANDLE,
    pub metadata: Metadata,
    pub current_offset: u32,
    pub start_cluster: Cluster,
    pub name: String
}

// FIXME: Implement `traits::File` (and its supertraits) for `File`.
impl<HANDLE: VFatHandle> traits::File for File<HANDLE> {
    /// Writes any buffered data to disk.
    fn sync(&mut self) -> io::Result<()> {
        unimplemented!()
    }

    /// Returns the size of the file in bytes.
    fn size(&self) -> u64 {
        self.metadata.size as u64
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
        use traits::File;
        let read_size = min(buf.len(), (self.size() - self.current_offset as u64) as usize);
        let bytes_read = self.vfat.lock(|vfat: &mut VFat<HANDLE>| -> io::Result<usize> {
            println!("read {}", read_size);
            println!("buf len {}", buf.len());
            let mut buf_vec: Vec<u8> = Vec::new();
            let read = vfat.read_chain_from_offset(self.start_cluster, self.current_offset as usize, &mut buf_vec)?;
            buf[..read_size].clone_from_slice(&buf_vec);
            Ok(read)
        })?;

        Ok(bytes_read)
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
        self.current_offset = match pos {
            SeekFrom::Start(off) => {
                if off > self.size() {
                    return ioerr!(InvalidInput, "Attempted to seek past end of file");
                }
                off as u32
            },
            SeekFrom::End(off) => {
                if self.size() as i64 + off  < 0 {
                    return ioerr!(InvalidInput, "Attempted to seek past beginning of file");
                }

                if off > 0 {
                    return ioerr!(InvalidInput, "Attempted to seek past end of file");
                }

                (self.size() as i64 + off) as u32
            },
            SeekFrom::Current(off) => {
                if self.current_offset as i64 + off < 0 {
                    return ioerr!(InvalidInput, "Attempted to seek past beginning of file");
                } 

                if self.current_offset as i64 + off > self.size() as i64 {
                    return ioerr!(InvalidInput, "Attempted to seek past end of file");
                }

                (self.current_offset as i64 + off) as u32
            }
        };
        
        Ok(self.current_offset as u64)
    }
}
