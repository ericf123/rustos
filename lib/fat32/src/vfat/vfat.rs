use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::size_of;

use alloc::vec::Vec;

use shim::io;
use shim::ioerr;
use shim::path::Path;
use shim::path::Component;
use core::cmp::min;

use crate::mbr::MasterBootRecord;
use crate::traits::{BlockDevice, FileSystem};
use crate::util::SliceExt;
use crate::vfat::{BiosParameterBlock, CachedPartition, Partition};
use crate::vfat::{Cluster, Dir, Error, FatEntry, Status, Metadata};

/// A generic trait that handles a critical section as a closure
pub trait VFatHandle: Clone + Debug + Send + Sync {
    fn new(val: VFat<Self>) -> Self;
    fn lock<R>(&self, f: impl FnOnce(&mut VFat<Self>) -> R) -> R;
}

#[derive(Debug)]
pub struct VFat<HANDLE: VFatHandle> {
    phantom: PhantomData<HANDLE>,
    pub device: CachedPartition,
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub sectors_per_fat: u32,
    pub fat_start_sector: u64,
    pub data_start_sector: u64,
    pub rootdir_cluster: Cluster,
}

impl<HANDLE: VFatHandle> VFat<HANDLE> {
    pub fn from<T>(mut device: T) -> Result<HANDLE, Error>
    where
        T: BlockDevice + 'static,
    {
        let mbr = MasterBootRecord::from(&mut device)?;
        let part = match mbr.partition_table.iter().find(|&partition| partition.partition_type == 0xB || partition.partition_type == 0xC) {
            Some(v) => v,
            None => return Err(Error::NotFound)
        };
        let ebpb = BiosParameterBlock::from(&mut device, part.relative_sector.into())?;
        // we want to consume the device here
        let cached_partition = CachedPartition::new(device, Partition { 
            start: part.relative_sector as u64, 
            num_sectors: part.total_sectors as u64, 
            sector_size: ebpb.bytes_per_sector as u64,

        });
        let fat_start_sector = cached_partition.partition.start + ebpb.num_reserved_sectors as u64;
        let data_start_sector = fat_start_sector + (ebpb.num_fats as u32 * ebpb.sectors_per_fat) as u64;
        let vfat = VFat::<HANDLE> {
            phantom: PhantomData,
            device: cached_partition,
            bytes_per_sector: ebpb.bytes_per_sector,
            sectors_per_cluster: ebpb.sectors_per_cluster,
            sectors_per_fat: ebpb.sectors_per_fat,
            fat_start_sector: fat_start_sector, 
            data_start_sector: data_start_sector,
            rootdir_cluster: Cluster::from(ebpb.root_cluster)
        };

        //println!("root cluster vfat {:#?}", &vfat.rootdir_cluster);
        Ok(HANDLE::new(vfat))
    }

    //
    //  * A method to read from an offset of a cluster into a buffer.
    pub fn read_cluster(&mut self, cluster: Cluster, offset: usize, buf: &mut [u8]) -> io::Result<usize> {
        let cluster_entry = self.fat_entry(cluster)?;
        match cluster_entry.status() {
            Status::Data(_) | Status::Eoc(_) => {
                let cluster_data_start_sector = self.data_start_sector + ((cluster.0 - 2) as u64 * self.sectors_per_cluster as u64);
                let mut cluster_data: Vec<u8> = Vec::new();
                for i in 0..self.sectors_per_cluster {
                    let start_idx = i as usize * self.bytes_per_sector as usize;
                    let end_idx = start_idx + self.bytes_per_sector as usize;
                    let curr_sector = cluster_data_start_sector + i as u64;
                    cluster_data.resize(cluster_data.len() + self.bytes_per_sector as usize, 0);
                    cluster_data[start_idx..end_idx].clone_from_slice(self.device.get_mut(curr_sector)?);
                }

                let num_bytes_read = min(buf.len(), (self.bytes_per_sector * self.sectors_per_cluster as u16) as usize - offset);
                buf[..num_bytes_read].clone_from_slice(&cluster_data[offset..offset + num_bytes_read]);

                Ok(num_bytes_read)
            },
            _ => ioerr!(InvalidData, "attempted to read non-data cluster")
        }
    }

    //
    //  * A method to read all of the clusters chained from a starting cluster
    //    into a vector.
    //
    pub fn read_chain(&mut self, start: Cluster, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.read_chain_from_offset(start, 0, buf)
    }

    pub fn read_chain_from_offset(&mut self, start: Cluster, offset: usize, buf: &mut Vec<u8>) -> io::Result<usize> {
        let bytes_per_cluster = (self.bytes_per_sector * self.sectors_per_cluster as u16) as usize;
        let mut curr_cluster = start;
        let mut num_bytes_read = 0;
        let mut cluster_offset = offset % bytes_per_cluster;
        loop {
            let curr_entry = self.fat_entry(curr_cluster)?;
            match curr_entry.status() {
                Status::Data(next) => {
                    // grow the buf to accomodate new cluster
                    buf.resize(buf.len() + bytes_per_cluster, 0);
                    // read the cluster into buf
                    self.read_cluster(curr_cluster, cluster_offset, &mut buf[num_bytes_read..num_bytes_read + bytes_per_cluster])?;
                    curr_cluster = next;
                },
                Status::Eoc(_marker) => {
                    // grow the buf to accomodate new cluster
                    buf.resize(buf.len() + bytes_per_cluster, 0);
                    // read the cluster into buf
                    self.read_cluster(curr_cluster, cluster_offset, &mut buf[num_bytes_read..num_bytes_read + bytes_per_cluster])?;
                    break;
                },
                _ => return ioerr!(UnexpectedEof, "got invalid entry status while reading cluster chain")
            };

            num_bytes_read += bytes_per_cluster;
            cluster_offset = 0;
        }

        Ok(num_bytes_read)
    }

    //
    //  * A method to return a reference to a `FatEntry` for a cluster where the
    //    reference points directly into a cached sector.
    //
    pub fn fat_entry(&mut self, cluster: Cluster) -> io::Result<&FatEntry> {
        let (sector, offset) = self.get_cluster_addr(cluster);
        let sector_data = unsafe { self.device.get(sector)?.cast::<FatEntry>() };
        Ok(&sector_data[offset / size_of::<FatEntry>()])
    }
        
    // converts cluster to a sector and an offset within that sector
    // inside the FAT
    fn get_cluster_addr(&self, cluster: Cluster) -> (u64, usize) {
        //let entries_per_sector = self.bytes_per_sector / 32;
        let sector = self.fat_start_sector + (cluster.0 * size_of::<FatEntry>() as u32 / self.bytes_per_sector as u32) as u64;
        let offset = cluster.0 * size_of::<FatEntry>() as u32 % self.bytes_per_sector as u32; 

        (sector, offset as usize)
    }
}

impl<'a, HANDLE: VFatHandle> FileSystem for &'a HANDLE {
    type File = crate::vfat::File<HANDLE>;
    type Dir = crate::vfat::Dir<HANDLE>;
    type Entry = crate::vfat::Entry<HANDLE>;

    fn open<P: AsRef<Path>>(self, path: P) -> io::Result<Self::Entry> {
        use crate::traits::Entry;
        let components = path.as_ref().components();
        let mut dir_entries: Vec<crate::vfat::Entry<HANDLE>> = Vec::new();

        for comp in components {
            match comp {
                Component::RootDir => {
                    dir_entries.truncate(0);
                    dir_entries.push(crate::vfat::Entry::Dir(Dir{
                        vfat: self.clone(),
                        metadata: Metadata::default(),
                        start_cluster: self.lock(|vfat: &mut VFat<HANDLE>| -> Cluster {
                            vfat.rootdir_cluster
                        }),
                        name: "root".to_owned()
                    }));
                }
                Component::Normal(name) => {
                    let new_entry = match dir_entries.last() {
                        Some(curr_entry) => {
                            match curr_entry.as_dir() {
                                Some(dir) => dir.find(name)?,
                                None => return ioerr!(NotFound, "file not found")
                            }
                        },
                        None => return ioerr!(NotFound, "file not found")
                    };

                    dir_entries.push(new_entry);
                },
                Component::ParentDir => {
                    if dir_entries.len() > 0 {
                        dir_entries.pop();
                    } else {
                        return ioerr!(NotFound, "file not found");
                    }
                },
                _ => (),
            }
        }

        let entry = match dir_entries.into_iter().last() {
            Some(e) => e,
            None => return ioerr!(NotFound, "file not found")
        };

        Ok(entry)
    }
}
