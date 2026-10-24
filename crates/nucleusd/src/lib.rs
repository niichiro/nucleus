//! Сервер Nucleus: устройства (виртуальные + Zigbee), HTTP API, MQTT-мост, mDNS.
//!
//! Роли: BLE-периферия — только с feature `ble-peripheral` (Linux/BlueZ);
//! в M3 появится BLE-централ (btleplug) для подключения к узлам mesh.

#[cfg(feature = "ble-peripheral")]
pub mod ble;
pub mod config;
pub mod devices;
pub mod http;
pub mod mdns;
pub mod mqtt;
pub mod state;
