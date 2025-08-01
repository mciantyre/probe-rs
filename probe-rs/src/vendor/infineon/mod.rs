//! Infineon vendor support.

use jep106::JEP106Code;
use probe_rs_target::{Chip, chip_detection::ChipDetectionMethod};

use crate::{
    architecture::arm::{
        ArmChipInfo, ArmDebugInterface, ArmError, FullyQualifiedApAddress,
        dp::{DpRegister, TARGETID},
        memory::ArmMemoryInterface,
    },
    config::{DebugSequence, Registry},
    error::Error,
    vendor::{Vendor, infineon::sequences::xmc4000::XMC4000},
};

pub mod sequences;

/// Infineon
#[derive(docsplay::Display)]
pub struct Infineon;

const INFINEON: JEP106Code = JEP106Code { id: 0x41, cc: 0x00 };
const CYPRESS: JEP106Code = JEP106Code { id: 0x34, cc: 0x00 };

impl Vendor for Infineon {
    fn try_create_debug_sequence(&self, chip: &Chip) -> Option<DebugSequence> {
        let sequence = if chip.name.starts_with("XMC4") {
            DebugSequence::Arm(XMC4000::create())
        } else {
            return None;
        };

        Some(sequence)
    }

    fn try_detect_arm_chip(
        &self,
        registry: &Registry,
        interface: &mut dyn ArmDebugInterface,
        chip_info: ArmChipInfo,
    ) -> Result<Option<String>, Error> {
        if let Some(psoc) = try_detect_psoc(registry, interface, &chip_info)? {
            Ok(Some(psoc))
        } else {
            try_detect_xmc4xxx(registry, interface, &chip_info)
        }
    }
}

fn try_detect_xmc4xxx(
    registry: &Registry,
    interface: &mut dyn ArmDebugInterface,
    chip_info: &ArmChipInfo,
) -> Result<Option<String>, Error> {
    if chip_info.manufacturer != INFINEON {
        return Ok(None);
    }

    const KNOWN_PARTS: &[u16] = &[0x1dd, 0x1df, 0x1dc, 0x1db];
    if !KNOWN_PARTS.contains(&chip_info.part) {
        return Ok(None);
    }

    // FIXME: This is a bit shaky but good enough for now.
    let access_port = &FullyQualifiedApAddress::v1_with_default_dp(0);
    let mut memory_interface = interface.memory_interface(access_port)?;

    // First, read the SCU peripheral ID register to verify that this is an XMC4000.
    let Some(scu_idchip) = read_xmc4xxx_scu_idchip(memory_interface.as_mut())? else {
        return Ok(None);
    };

    tracing::debug!("SCU_IDCHIP = {:#010x}", scu_idchip);

    // The MCU does not tell us its flash size, so we have to probe for it. For this, we are
    // reading suspected the last words of the uncached flash memory.
    let flash_size_kb = probe_xmc4xxx_flash_size(0x0c00_0000, memory_interface.as_mut());

    // Now look up a closest match. We are not able to tell exactly which device this is, because
    // the identical die is packaged up differently for different devices.

    for family in registry.families() {
        for info in family
            .chip_detection
            .iter()
            .filter_map(ChipDetectionMethod::as_infineon_xmc_scu)
        {
            if info.part != chip_info.part || info.scu_id != (scu_idchip & 0xFFFF0) >> 4 {
                continue;
            }

            for (flash, variant) in info.variants.iter() {
                if *flash == flash_size_kb {
                    return Ok(Some(variant.clone()));
                }
            }
        }
    }

    Ok(None)
}

fn read_xmc4xxx_scu_idchip(memory: &mut dyn ArmMemoryInterface) -> Result<Option<u32>, ArmError> {
    // The SCU peripheral has a peripheral/module ID register:
    bitfield::bitfield! {
        /// SCU->ID register.
        #[derive(Copy,Clone)]
        struct ScuId(u32);
        impl Debug;
        pub mod_rev, _: 7, 0;
        pub mod_type, _: 15, 8;
        pub mod_number, _: 31, 16;
    }
    impl ScuId {
        const ADDRESS: u32 = 0x5000_4000;
    }

    // And it has a chip ID register:
    #[derive(Debug, Copy, Clone)]
    struct ScuChipId;
    impl ScuChipId {
        const ADDRESS: u32 = 0x5000_4004;
    }

    // Read the SCU ID
    let scu_id = ScuId(memory.read_word_32(ScuId::ADDRESS as u64)?);
    if scu_id.mod_type() != 0xC0 {
        return Ok(None);
    }

    // Read the SCU chip ID register
    memory.read_word_32(ScuChipId::ADDRESS as u64).map(Some)
}

fn probe_xmc4xxx_flash_size(start_addr: u32, memory: &mut dyn ArmMemoryInterface) -> u32 {
    let mut last_successful_size = 0;
    // TODO: if we need to be more general, implement a binary search here.
    for size in [
        // Actual flash sizes used in XMC4xxx devices
        64, 128, 256, 512, 768, 1024, 1536, 2048,
        // So we can detect "all reads succeeded", which shouldn't happen
        2049,
    ] {
        let addr = start_addr + (size * 1024) - 4;
        if memory.read_word_32(addr as u64).is_err() {
            break;
        }
        last_successful_size = size;
    }
    last_successful_size
}

fn try_detect_psoc(
    registry: &Registry,
    interface: &mut dyn ArmDebugInterface,
    chip_info: &ArmChipInfo,
) -> Result<Option<String>, Error> {
    if chip_info.manufacturer != INFINEON && chip_info.manufacturer != CYPRESS {
        return Ok(None);
    }

    let mut families = registry
        .families()
        .iter()
        .filter(|f| f.manufacturer == Some(chip_info.manufacturer))
        .flat_map(|f| f.chip_detection.iter())
        .flat_map(ChipDetectionMethod::as_infineon_psoc_siid)
        .filter(|f| f.family_id == chip_info.part)
        .peekable();

    if families.peek().is_none() {
        return Ok(None);
    }

    let tid = TARGETID(
        interface
            .read_raw_dp_register(interface.current_debug_port().unwrap(), TARGETID::ADDRESS)?,
    );
    let siid = tid.tpartno();

    Ok(families.find_map(|family| family.silicon_ids.get(&siid).cloned()))
}
