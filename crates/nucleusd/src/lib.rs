//! Сервер Nucleus: устройства (Zigbee через zigbee2mqtt), HTTP API, MQTT-мост, mDNS.
//!
//! BLE: периферия — feature `ble-peripheral` (Linux/BlueZ);
//! централ (подключение к узлам mesh как телефон) — feature `ble-central`.

#[cfg(feature = "ble-peripheral")]
pub mod ble;
#[cfg(feature = "ble-central")]
pub mod ble_central;
pub mod config;
pub mod devices;
pub mod http;
pub mod mdns;
pub mod mqtt;
