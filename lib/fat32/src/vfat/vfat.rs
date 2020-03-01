use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem::size_of;

use alloc::vec::Vec;

use shim::io;
use shim::ioerr;
use shim::newioerr;
use shim::path;
use shim::path::Path;
use shim::path::Component;
use core::cmp::min;

use crate::mbr::MasterBootRecord;
use crate::traits::{BlockDevice, FileSystem};
use crate::util::SliceExt;
use crate::vfat::{BiosParameterBlock, CachedPartition, Partition};
use crate::vfat::{Cluster, Dir, Entry, Error, FatEntry, File, Status, Metadata};

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
                // TODO do without double buf?
                /*if cluster.0 < 2 {
                    println!("reading cluster {}", cluster.0);
                }*/
                let cluster_data_start_sector = self.data_start_sector + ((cluster.0 - 2) as u64 * self.sectors_per_cluster as u64);
                let mut cluster_data: Vec<u8> = Vec::new();
                for i in 0..self.sectors_per_cluster {
                    let start_idx = i as usize * self.bytes_per_sector as usize;
                    let end_idx = start_idx + self.bytes_per_sector as usize;
                    let curr_sector = cluster_data_start_sector + i as u64;
                    //println!("curr sector {}", curr_sector);
                    cluster_data.resize(cluster_data.len() + self.bytes_per_sector as usize, 0);
                    cluster_data[start_idx..end_idx].clone_from_slice(self.device.get_mut(curr_sector)?);
                }

                let num_bytes_read = min(buf.len(), (self.bytes_per_sector * self.sectors_per_cluster as u16) as usize - offset);
                /*println!("bytes read {}", num_bytes_read);
                println!("cluster data len {}", cluster_data.len());
                println!("offset {}", offset);*/
                buf[..num_bytes_read].clone_from_slice(&cluster_data[offset..offset + num_bytes_read]);

                Ok(num_bytes_read)
            },
            _ => ioerr!(InvalidData, "attempted to read non-data cluster")
        }
    }

    pub(super) fn read_cluster_(
        &mut self,
        cluster: Cluster,
        offset: usize,
        buf: &mut [u8],
    ) -> io::Result<usize> {
        println!("cluster #{}", cluster.0);
        if !cluster.is_valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid cluster",
            ));
        }

        let sector_size = self.device.sector_size() as usize;
        let size = min(
            buf.len(),
            self.bytes_per_sector as usize * self.sectors_per_cluster as usize - offset,
        );
        let mut current_sector = self.data_start_sector
            + cluster.cluster_index() as u64 * self.sectors_per_cluster as u64
            + offset as u64 / self.bytes_per_sector as u64;
        let mut bytes_read = 0;
        let mut offset_once = offset % self.bytes_per_sector as usize;
        while bytes_read < size {
            let content = self.device.get(current_sector)?;
            let copy_size = min(size - bytes_read, sector_size - offset_once);
            buf[bytes_read..bytes_read + copy_size]
                .copy_from_slice(&content[offset_once..offset_once + copy_size]);
            offset_once = 0;
            bytes_read += copy_size;
            current_sector += 1;
        }

        Ok(size)
    }

    //
    //  * A method to read all of the clusters chained from a starting cluster
    //    into a vector.
    //
    pub fn read_chain(&mut self, start: Cluster, buf: &mut Vec<u8>) -> io::Result<usize> {
        self.read_chain_from_offset(start, 0, buf)
    }

     pub(super) fn fat_entry_(&mut self, cluster: Cluster) -> io::Result<&FatEntry> {
        let cluster_num_sector: u64 = cluster.cluster_num() as u64 * size_of::<FatEntry>() as u64
            / self.bytes_per_sector as u64;
        let entry_offset: usize =
            cluster.cluster_num() as usize * size_of::<FatEntry>() % self.bytes_per_sector as usize;
        let content = self.device.get(self.fat_start_sector + cluster_num_sector)?;
        let entries: &[FatEntry] = unsafe { content.cast() };
        Ok(&entries[entry_offset / size_of::<FatEntry>()])
    }

    pub(super) fn read_chain_(&mut self, start: Cluster, buf: &mut Vec<u8>) -> io::Result<usize> {
        // Floyd's Cycle Detection Algorithm
        // This is the tortoise
        let mut current_cluster = start;
        // This is the hare
        let mut hare_cluster = self.next_cluster(current_cluster)?;
        let mut current_cluster_num = 0;
        let bytes_per_cluster = self.bytes_per_sector as usize * self.sectors_per_cluster as usize;
        while Some(current_cluster) != hare_cluster {
            current_cluster_num += 1;
            buf.resize(bytes_per_cluster * current_cluster_num, 0);
            self.read_cluster(
                current_cluster,
                0,
                &mut buf[bytes_per_cluster * (current_cluster_num - 1)..],
            )?;
            match self.next_cluster(current_cluster)? {
                Some(next_cluster) => {
                    current_cluster = next_cluster;
                }
                None => {
                    return Ok(bytes_per_cluster * current_cluster_num);
                }
            }
            if let Some(cluster) = hare_cluster {
                hare_cluster = self.next_cluster(cluster)?;
            }
            if let Some(cluster) = hare_cluster {
                hare_cluster = self.next_cluster(cluster)?;
            }
        }
        Err(io::Error::new(io::ErrorKind::InvalidData, "Cycle detected in cluster chain"))
    }

    fn next_cluster(&mut self, cluster: Cluster) -> io::Result<Option<Cluster>> {
        if !cluster.is_valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid cluster num",
            ));
        }
        match self.fat_entry(cluster)?.status() {
            Status::Eoc(_) => Ok(None),
            Status::Data(next_cluster) => {
                if next_cluster.is_valid() {
                    Ok(Some(next_cluster))
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid cluster num",
                    ))
                }
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid cluster chain",
            )),
        }
    }

    pub fn read_chain_from_offset(&mut self, start: Cluster, offset: usize, buf: &mut Vec<u8>) -> io::Result<usize> {
        let bytes_per_cluster = (self.bytes_per_sector * self.sectors_per_cluster as u16) as usize;
        let mut curr_cluster = start;//Cluster::from(start.0 + (offset / bytes_per_cluster) as u32);
        let mut buf_offset = 0;
        let mut num_bytes_read = 0;
        //let first_cluster_num = offset / bytes_per_cluster;
        let mut curr_cluster_num = 0;
        let mut load_buf = true;
        let mut cluster_offset = offset % bytes_per_cluster;
        loop {
            /*if curr_cluster_num == first_cluster_num {
                load_buf = true;
            }*/

            /*let cluster_offset = match curr_cluster_num {
                first_cluster_num => offset % bytes_per_cluster,
                _ => 0
            };*/

            /*let mut cluster_offset = 0;
            if curr_cluster_num == first_cluster_num {
                cluster_offset = offset % bytes_per_cluster; 
            }*/

            let curr_entry = self.fat_entry(curr_cluster)?;
            //println!("reading cluster: {:#?}", curr_cluster);
            match curr_entry.status() {
                Status::Data(next) => {
                    if load_buf {
                        // grow the buf to accomodate new cluster
                        buf.resize(buf.len() + bytes_per_cluster, 0);
                        // read the cluster into buf
                        self.read_cluster(curr_cluster, cluster_offset, &mut buf[num_bytes_read..num_bytes_read + bytes_per_cluster])?;
                    }
                    curr_cluster = next;
                },
                Status::Eoc(marker) => {
                    //println!("{} EOC!", curr_cluster_num);
                    if load_buf {
                        // grow the buf to accomodate new cluster
                        buf.resize(buf.len() + bytes_per_cluster, 0);
                        // read the cluster into buf
                        self.read_cluster(curr_cluster, cluster_offset, &mut buf[num_bytes_read..num_bytes_read + bytes_per_cluster])?;
                    }
                    break;
                },
                _ => return ioerr!(UnexpectedEof, "got invalid entry status while reading cluster chain")
            };

            num_bytes_read += bytes_per_cluster;
            curr_cluster_num += 1;
            buf_offset += bytes_per_cluster;
            cluster_offset = 0;
        }

        Ok(num_bytes_read)
        /*let bytes_per_cluster = (self.bytes_per_sector * self.sectors_per_cluster as u16) as usize;
        let mut curr_cluster = start;//Cluster::from(start.0 + (offset / bytes_per_cluster) as u32);
        //println!("start cluster: {}", curr_cluster.0);
        let mut cluster_offset = offset % bytes_per_cluster;
        let mut num_bytes_read = 0;
        loop {
            let curr_entry = self.fat_entry(curr_cluster)?;
            //println!("reading cluster: {:#?}", curr_cluster);
            match curr_entry.status() {
                Status::Data(next) => {
                    // grow the buf to accomodate new cluster
                    buf.resize(buf.len() + bytes_per_cluster, 0);
                    // read the cluster into buf
                    num_bytes_read += self.read_cluster(curr_cluster, cluster_offset, &mut buf[num_bytes_read..num_bytes_read + bytes_per_cluster])?;
                    curr_cluster = next;
                },
                Status::Eoc(marker) => {
                    //println!("{} EOC!", curr_cluster_num);
                    // grow the buf to accomodate new cluster
                    buf.resize(buf.len() + bytes_per_cluster, 0);
                    // read the cluster into buf
                    num_bytes_read += self.read_cluster(curr_cluster, cluster_offset, &mut buf[num_bytes_read..num_bytes_read + bytes_per_cluster])?;
                    break;
                },
                _ => return ioerr!(UnexpectedEof, "got invalid entry status while reading cluster chain")
            };
            cluster_offset = 0;
        }
        Ok(num_bytes_read)*/
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
        let mut components = path.as_ref().components();
        //assert_eq!(components.next(), Some(Component::RootDir));

        //let curr_fat_entry = self.fat_entry(self.root_cluster);
        /*let mut curr_entry: Entry<HANDLE> = crate::vfat::Entry::Dir(Dir {
            vfat: self,
            metadata: Metadata::default(),
            start_cluster: self.lock(|vfat: &mut VFat<HANDLE>| -> Cluster {
                vfat.rootdir_cluster
            })
        // });*/

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
