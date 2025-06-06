//! Types and functions for interacting with v1 access ports.

use crate::architecture::arm::{
    DapAccess, FullyQualifiedApAddress,
    ap::{ApAccess, ApRegister, GenericAp, IDR},
    dp::DpAddress,
};

/// Return a Vec of all valid access ports found that the target connected to the debug_probe.
/// Can fail silently under the hood testing an ap that doesn't exist and would require cleanup.
#[tracing::instrument(skip(debug_port))]
pub(crate) fn valid_access_ports<DP>(
    debug_port: &mut DP,
    dp: DpAddress,
) -> Vec<FullyQualifiedApAddress>
where
    DP: DapAccess,
{
    valid_access_ports_allowlist(debug_port, dp, 0..=255)
}

/// Determine if an AP exists with the given AP address.
///
/// The test is performed by reading the IDR register, and checking if the register is non-zero.
///
/// Can fail silently under the hood testing an ap that doesn't exist and would require cleanup.
pub fn access_port_is_valid<AP>(
    debug_port: &mut AP,
    access_port: &FullyQualifiedApAddress,
) -> Option<IDR>
where
    AP: DapAccess,
{
    let idr_result: Result<IDR, _> = debug_port
        .read_raw_ap_register(access_port, IDR::ADDRESS)
        .and_then(|idr| Ok(IDR::try_from(idr)?));

    match idr_result {
        Ok(idr) if u32::from(idr) != 0 => Some(idr),
        Ok(_) => {
            tracing::debug!("AP {:?} is not valid, IDR = 0", access_port.ap());
            None
        }
        Err(e) => {
            tracing::debug!(
                "Error reading IDR register from AP {:?}: {}",
                access_port.ap(),
                e
            );
            None
        }
    }
}

/// Return a Vec of all valid access ports found that the target connected to the debug_probe.
/// The search is limited to `allowed_aps`.
///
/// Can fail silently under the hood testing an ap that doesn't exist and would require cleanup.
#[tracing::instrument(skip(debug_port, allowed_aps))]
pub(crate) fn valid_access_ports_allowlist<DP>(
    debug_port: &mut DP,
    dp: DpAddress,
    allowed_aps: impl IntoIterator<Item = u8>,
) -> Vec<FullyQualifiedApAddress>
where
    DP: DapAccess,
{
    allowed_aps
        .into_iter()
        .map_while(|ap| {
            let ap = FullyQualifiedApAddress::v1_with_dp(dp, ap);
            access_port_is_valid(debug_port, &ap).map(|_| ap)
        })
        .collect()
}

/// Tries to find the first AP with the given idr value, returns `None` if there isn't any
pub fn get_ap_by_idr<AP, P>(debug_port: &mut AP, dp: DpAddress, f: P) -> Option<GenericAp>
where
    AP: ApAccess,
    P: Fn(IDR) -> bool,
{
    (0..=255)
        .map(|ap| GenericAp::new(FullyQualifiedApAddress::v1_with_dp(dp, ap)))
        .find(|ap| {
            if let Ok(idr) = debug_port.read_ap_register(ap) {
                f(idr)
            } else {
                false
            }
        })
}
