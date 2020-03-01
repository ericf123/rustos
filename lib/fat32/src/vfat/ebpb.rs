use core::fmt;
use core::mem;
use core::str;
use shim::const_assert_size;

use crate::traits::BlockDevice;
use crate::vfat::Error;

#[repr(C, packed)]
pub struct BiosParameterBlock {
    _ebxx90: [u8; 3],
    pub oem_id: u64,
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub num_reserved_sectors: u16,
    pub num_fats: u8,
    pub max_dirs: u16,
    _total_logical_sectors: u16,
    pub fat_id: u8,
    _sectors_per_fat: u16,
    sectors_per_track: u16,
    pub num_heads: u16, 
    pub num_hidden_sectors: u32,
    pub total_logical_sectors: u32,
    pub sectors_per_fat: u32,
    pub flags: u16,
    pub fat_version: u16,
    pub root_cluster: u32,
    pub fs_info_sector: u16,
    pub backup_boot_sector: u16,
    _reserved0: [u8; 12],
    pub drive_num: u8,
    _reserved1: u8,
    pub ebpb_signature: u8,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
    pub system_id: [u8; 8],
    pub boot_code: [u8; 420],
    pub boot_signature: u16,
}

const_assert_size!(BiosParameterBlock, 512);

impl BiosParameterBlock {
    /// Reads the FAT32 extended BIOS parameter block from sector `sector` of
    /// device `device`.
    ///
    /// # Errors
    ///
    /// If the EBPB signature is invalid, returns an error of `BadSignature`.
    pub fn from<T: BlockDevice>(mut device: T, sector: u64) -> Result<BiosParameterBlock, Error> {
        // read the EBPB from the block device, 
        // check for IO errors
        let mut ebpb_buf = [0u8; 512];
        match device.read_sector(sector, &mut ebpb_buf) {
             Err(err) => return Err(Error::Io(err)),
            _ => ()
        };

        // transmute the buffer into an EBPB
        let mut ebpb = unsafe { mem::transmute::<[u8; 512], BiosParameterBlock>(ebpb_buf) };
        
        // validate signatures
        if !(ebpb.ebpb_signature == 0x28 || ebpb.ebpb_signature == 0x29)
             || ebpb.boot_signature != 0xAA55 {
            
            return Err(Error::BadSignature);
        }

        if ebpb._total_logical_sectors != 0 {
            ebpb.total_logical_sectors = ebpb._total_logical_sectors as u32;
        }
        Ok(ebpb)
    }
}

impl fmt::Debug for BiosParameterBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "OEM ID               {:?}\n\
                   BYTES PER SECTOR     {:?}\n\
                   SECTORS PER CLUSTER  {:?}\n\
                   NUM RESERVED SECTORS {:?}\n\
                   NUM FATs             {:?}\n\
                   MAX DIRS             {:?}\n\
                   NUM LOGICAL SECTORS  {:?}\n\
                   FAT ID               {:?}\n\
                   SECTORS PER FAT      {:?}\n\
                   SECTORS PER TRACK    {:?}\n\
                   NUM HEADS            {:?}\n\
                   NUM HIDDEN SECTORS   {:?}\n\
                   FLAGS                {:?}\n\
                   FAT VERSION          {:?}\n\
                   ROOT CLUSTER         {:#x}\n\
                   FSINFO SECTOR        {:#x}\n\
                   BACKUP BOOT SECTOR   {:#x}\n\
                   DRIVE NUMBER         {:?}\n\
                   SIGNATURE            {:#x}\n\
                   VOLUME ID            {:?}\n\
                   VOLUME LABEL         {:?}\n\
                   SYSTEM IDENTIFIER    {:?}\n\
                   BOOT SIGNATURE       {:#x}\n\
                   ",
                   &{self.oem_id}, &{self.bytes_per_sector}, &{self.sectors_per_cluster},
                   &{self.num_reserved_sectors}, &{self.num_fats}, &{self.max_dirs},
                   &{self.total_logical_sectors}, &{self.fat_id}, &{self.sectors_per_fat},
                   &{self.sectors_per_track}, &{self.num_heads}, &{self.num_hidden_sectors}, 
                   &{self.flags}, &{self.fat_version}, &{self.root_cluster}, &{self.fs_info_sector}, 
                   &{self.backup_boot_sector}, &{self.drive_num}, &{self.ebpb_signature}, &{self.volume_id},
                   str::from_utf8(&{self.volume_label}), str::from_utf8(&{self.system_id}), &{self.boot_signature})
    }
}
