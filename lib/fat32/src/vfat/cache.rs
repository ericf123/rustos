use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use hashbrown::HashMap;
use shim::io;
use shim::ioerr;

use crate::traits::BlockDevice;

#[derive(Debug)]
struct CacheEntry {
    data: Vec<u8>,
    dirty: bool,
}

pub struct Partition {
    /// The physical sector where the partition begins.
    pub start: u64,
    /// Number of sectors
    pub num_sectors: u64,
    /// The size, in bytes, of a logical sector in the partition.
    pub sector_size: u64,
}

pub struct CachedPartition {
    device: Box<dyn BlockDevice>,
    cache: HashMap<u64, CacheEntry>,
    pub partition: Partition,
}

impl CachedPartition {
    /// Creates a new `CachedPartition` that transparently caches sectors from
    /// `device` and maps physical sectors to logical sectors inside of
    /// `partition`. All reads and writes from `CacheDevice` are performed on
    /// in-memory caches.
    ///
    /// The `partition` parameter determines the size of a logical sector and
    /// where logical sectors begin. An access to a sector `0` will be
    /// translated to physical sector `partition.start`. Virtual sectors of
    /// sector number `[0, num_sectors)` are accessible.
    ///
    /// `partition.sector_size` must be an integer multiple of
    /// `device.sector_size()`.
    ///
    /// # Panics
    ///
    /// Panics if the partition's sector size is < the device's sector size.
    pub fn new<T>(device: T, partition: Partition) -> CachedPartition
    where
        T: BlockDevice + 'static,
    {
        assert!(partition.sector_size >= device.sector_size());

        CachedPartition {
            device: Box::new(device),
            cache: HashMap::new(),
            partition: partition,
        }
    }

    /// Returns the number of physical sectors that corresponds to
    /// one logical sector.
    fn factor(&self) -> u64 {
        self.partition.sector_size / self.device.sector_size()
    }

    /// Maps a user's request for a sector `virt` to the physical sector.
    /// Returns `None` if the virtual sector number is out of range.
    fn virtual_to_physical(&self, virt: u64) -> Option<u64> {
        //println!("virt sector {}", virt);
        //println!("num sectors {}", self.partition.num_sectors);
        if virt >= self.partition.num_sectors {
            return None;
        }

        let physical_offset = virt * self.factor();
        let physical_sector = self.partition.start + physical_offset;

        Some(physical_sector)
    }

    /// Returns a mutable reference to the cached sector `sector`. If the sector
    /// is not already cached, the sector is first read from the disk.
    ///
    /// The sector is marked dirty as a result of calling this method as it is
    /// presumed that the sector will be written to. If this is not intended,
    /// use `get()` instead.
    ///
    /// # Errors
    ///
    /// Returns an error if there is an error reading the sector from the disk.
    pub fn get_mut(&mut self, sector: u64) -> io::Result<&mut [u8]> {
        self.insert_if_not_exists(sector)?;

        let entry = match self.cache.get_mut(&sector) {
            Some(e) => e,
            None => return ioerr!(NotFound, "sector not found in cache in get_mut (this should never happen)")
        };
        entry.dirty = true;

        Ok(&mut entry.data)
    }

    /// Returns a reference to the cached sector `sector`. If the sector is not
    /// already cached, the sector is first read from the disk.
    ///
    /// # Errors
    ///
    /// Returns an error if there is an error reading the sector from the disk.
    pub fn get(&mut self, sector: u64) -> io::Result<&[u8]> {
        self.insert_if_not_exists(sector)?;

        let entry = match self.cache.get(&sector) {
            Some(e) => e,
            None => return ioerr!(NotFound, "sector not found in cache in get_mut (this should never happen)")
        };

        Ok(&entry.data)
    }

    /// Reads a logical sector from device and inserts it into the cache if 
    /// the sector is not already in the cache.
    fn insert_if_not_exists(&mut self, sector: u64) -> io::Result<()> {
        match self.virtual_to_physical(sector) {
            Some(physical_sector) => {
                if !self.cache.contains_key(&sector) {
                    // create a buf to hold virtual sector
                    let mut buf_vec: Vec<u8> = Vec::new();

                    // read the physical sectors that map to the requested virtual sector
                    for i in 0..self.factor() {
                        let phys_start_idx = (i * self.device.sector_size()) as usize;
                        let phys_end_idx = phys_start_idx + self.device.sector_size() as usize;
                        buf_vec.resize(buf_vec.len() + self.device.sector_size() as usize, 0);
                        //self.device.read_sector(physical_sector, &mut (&mut buf_vec)[phys_start_idx..phys_end_idx-1])?;
                        self.device.read_sector(physical_sector, &mut buf_vec[phys_start_idx..phys_end_idx])?;
                    }

                    // insert virtual sector in to cache
                    self.cache.insert(sector, CacheEntry { dirty: false, data: buf_vec });
                }             
                return Ok(());
            },
            None => return ioerr!(NotFound, "virtual sector number out of range")
        };
    }
}

// FIXME: Implement `BlockDevice` for `CacheDevice`. The `read_sector` and
// `write_sector` methods should only read/write from/to cached sectors.
impl BlockDevice for CachedPartition {
    fn sector_size(&self) -> u64 {
        self.partition.sector_size
    }

    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> io::Result<usize> {
        match self.get(sector) {
            Ok(read_buf) => {
                if buf.len() >= read_buf.len() {
                    // clone from slice panics if they aren't the same length
                    buf[..read_buf.len()].clone_from_slice(read_buf);
                    return Ok(read_buf.len());
                } else {
                    return ioerr!(UnexpectedEof, "destination buffer too small when reading from cached partition")
                }
            },
            Err(err) => return Err(err)
        };
    }

    fn write_sector(&mut self, sector: u64, buf: &[u8]) -> io::Result<usize> {
        match self.get_mut(sector) {
            Ok(write_buf) => {
                if write_buf.len() >= buf.len() {
                    // clone from slice panics if they aren't the same length
                    write_buf[..buf.len()].clone_from_slice(buf);
                    return Ok(buf.len());
                } else {
                    return ioerr!(UnexpectedEof, "destination buffer too small when writing to cached partition")
                }
            },
            Err(err) => return Err(err)
        };
    }
}

impl fmt::Debug for CachedPartition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("CachedPartition")
            .field("device", &"<block device>")
            .field("cache", &self.cache)
            .finish()
    }
}
