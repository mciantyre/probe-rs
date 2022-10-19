//! Information about all built-in targets supported by probe-rs.

use crate::DynError;
use probe_rs::config::{self, ChipFamily, MemoryRegion};

/// Print statistics about built-in probe-rs targets.
pub fn print() -> Result<(), DynError> {
    let families = config::families()?;
    memory_map_stats(families.iter())?;
    Ok(())
}

/// Produces stats about memory maps.
fn memory_map_stats<'a>(families: impl Iterator<Item = &'a ChipFamily>) -> Result<(), DynError> {
    let memory_regions = families
        .flat_map(|family| family.variants.iter())
        .flat_map(|variant| {
            variant
                .memory_map
                .iter()
                .map(|memory_region| (&variant.name, memory_region))
        });
    let ram_regions = memory_regions.filter_map(|(name, memory_region)| match memory_region {
        MemoryRegion::Ram(ram_region) => Some((name, ram_region)),
        _ => None,
    });
    let ram_sizes =
        ram_regions.map(|(name, ram_region)| (name, ram_region.range.end - ram_region.range.start));
    let (name, smallest_ram_size) = ram_sizes
        .min_by_key(|(_, size)| *size)
        .ok_or_else(|| "There's no ram regions?")?;

    println!("The {name} has one of the smallest RAM regions, spanning {smallest_ram_size} ({smallest_ram_size:#010X}) bytes.");
    println!("(Other chips may have the same smallest RAM size.)");

    Ok(())
}
