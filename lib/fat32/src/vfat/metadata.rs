use core::fmt;

use alloc::string::String;

use crate::traits;
use crate::vfat::Cluster;

/// A date as represented in FAT32 on-disk structures.
#[repr(C, packed)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Date(u16);

/// Time as represented in FAT32 on-disk structures.
#[repr(C, packed)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Time(u16);

/// File attributes as represented in FAT32 on-disk structures.
#[repr(C, packed)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Attributes(pub u8);

/// A structure containing a date and time.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub date: Date,
    pub time: Time,
}

/// Metadata for a directory entry.
#[derive(Default, Debug, Clone)]
pub struct Metadata {
    pub filename: String,
    pub extension: String,
    pub attributes: Attributes,
    pub created_timestamp: Timestamp,
    pub modified_timestamp: Timestamp,
    pub accessed_date: Timestamp,
    pub size: u32,
    pub start_cluster: Cluster
}

impl traits::Timestamp for Timestamp {
    fn year(&self) -> usize {
        ((self.date.0 >> 9) & 0x7F) as usize + 1980
    }

    fn month(&self) -> u8 {
        ((self.date.0 >> 5) & 0xF) as u8
    }

    fn day(&self) -> u8 {
        (self.date.0 & 0x1F) as u8
    }

    fn hour(&self) -> u8 {
        ((self.time.0 >> 11) & 0x1F) as u8
    }

    fn minute(&self) -> u8 {
        ((self.time.0 >> 5) & 0x3F) as u8
    }

    fn second(&self) -> u8 {
        (self.time.0 & 0x1F) as u8 * 2
    }
}

impl traits::Metadata for Metadata {
    /// Type corresponding to a point in time.
    type Timestamp = Timestamp;

    /// Whether the associated entry is read only.
    fn read_only(&self) -> bool {
        (self.attributes.0 & 0x01) != 0
    }

    /// Whether the entry should be "hidden" from directory traversals.
    fn hidden(&self) -> bool {
        (self.attributes.0 & 0x02) != 0
    }

    /// The timestamp when the entry was created.
    fn created(&self) -> Self::Timestamp {
        self.created_timestamp
    }

    /// The timestamp for the entry's last access.
    fn accessed(&self) -> Self::Timestamp {
        self.accessed_date
    }

    /// The timestamp for the entry's last modification.
    fn modified(&self) -> Self::Timestamp {
        self.modified_timestamp
    }
}

impl Metadata {
    fn drwh_str(&self) -> String {
        use traits::Metadata;
        let d = match self.attributes.0 & 0xF0 {
            0x10 => "d",
            _ => "-"
        };

        let w = match self.read_only() {
            true => "-",
            false => "w"
        };

        let h = match self.hidden() {
            true => "h",
            false => "-"
        };

        format!("{}r{}{}", d, w, h)
    } 
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use traits::Timestamp;
        write!(f, "{}/{}/{} {}:{}:{}", self.month(), self.day(), self.year(),
               self.hour(), self.minute(), self.second())
    }
}

impl fmt::Display for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {} {} {}", self.drwh_str(), self.created_timestamp, 
              self.accessed_date, self.modified_timestamp, self.filename)
    }
}
