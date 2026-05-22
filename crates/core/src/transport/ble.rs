use std::sync::Arc;

use iroh_base::EndpointId;
use iroh_ble_transport::BleTransport;
use tracing::{info, warn};

/// Try to build a BLE transport for the given endpoint ID.
/// Returns `None` if BLE hardware is unavailable.
pub async fn try_build_ble(endpoint_id: EndpointId) -> Option<Arc<BleTransport>> {
    match BleTransport::builder().build(endpoint_id).await {
        Ok(ble) => {
            info!("BLE transport started");
            Some(ble)
        }
        Err(e) => {
            warn!("BLE transport unavailable: {e}");
            None
        }
    }
}
