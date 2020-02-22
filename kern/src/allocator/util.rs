/// Align `addr` downwards to the nearest multiple of `align`.
///
/// The returned usize is always <= `addr.`
///
/// # Panics
///
/// Panics if `align` is not a power of 2.
pub fn align_down(addr: usize, align: usize) -> usize {
    if !align.is_power_of_two() {
        panic!("align must be power of two!!");
    }

    addr - (addr % align)
}

/// Align `addr` upwards to the nearest multiple of `align`.
///
/// The returned `usize` is always >= `addr.`
///
/// # Panics
///
/// Panics if `align` is not a power of 2
/// or aligning up overflows the address.
pub fn align_up(addr: usize, align: usize) -> usize {
    if !align.is_power_of_two() {
        panic!("align must be power of two!!");
    }
    let padding;
    if addr % align == 0{
        padding = 0;
    } else {
        padding = align - addr % align;
    }
    
    addr.checked_add(padding).unwrap() 
}
