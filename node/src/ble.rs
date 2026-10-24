//! BLE-периферия узла: GATT-профиль Nucleus поверх trouble-host.
//!
//! Одна характеристика-«кадр»: телефон/ПК пишет в неё команды (write),
//! узел уведомляет ответами (notify). Тот же профиль, что у сервера.

use alloc::vec::Vec;

use embassy_futures::join::join;
use embassy_futures::select::select;
use heapless::Vec as HVec;
use trouble_host::prelude::*;

use crate::engine;

/// Одновременных подключений (телефон + ПК).
const CONNECTIONS_MAX: usize = 2;
/// Каналов L2CAP.
const L2CAP_CHANNELS_MAX: usize = 2;
/// Максимальный кадр протокола (MTU с запасом).
const FRAME_MAX: usize = 180;

/// GATT-сервер Nucleus.
#[gatt_server]
struct Server {
    nucleus: NucleusService,
}

/// Сервис Nucleus (UUID как у сервера/ctl).
#[gatt_service(uuid = "a6c6a000-f18e-4bfd-a51a-c609e45263d1")]
struct NucleusService {
    /// Кадр протокола: [magic "NUC", version, postcard-payload].
    #[characteristic(uuid = "d6c6a000-f18e-4bfd-a51a-c609e45263d1", write, read, notify)]
    frame: HVec<u8, FRAME_MAX>,
}

/// Запуск BLE-стека и цикла «реклама → обслуживание соединения».
pub async fn run<C>(controller: C)
where
    C: Controller,
{
    // Фиксированный адрес — как в примерах trouble; заменим на MAC платы
    let address: Address = Address::random([0xff, 0x8f, 0x1a, 0x05, 0xe4, 0xff]);
    log::info!("BLE address = {address:?}");

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(address)
        .build();
    let runner = stack.runner();
    let mut peripheral = stack.peripheral();

    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "nu",
        appearance: &appearance::power_device::GENERIC_POWER_DEVICE,
    }))
    .expect("gatt server");

    let _ = join(ble_task(runner), async {
        loop {
            match advertise(&mut peripheral, &server).await {
                Ok(conn) => {
                    log::info!("BLE: central connected");
                    let events = gatt_events_task(&server, &conn);
                    let outgoing = notify_task(&server, &conn);
                    select(events, outgoing).await;
                    log::info!("BLE: connection closed");
                }
                Err(e) => {
                    log::warn!("BLE advertise: {e:?}");
                }
            }
        }
    })
    .await;
}

/// Фоновая задача BLE-стека (обязательна, крутится вечно).
async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if let Err(e) = runner.run().await {
            log::error!("ble_task: {e:?}");
        }
    }
}

/// Реклама «nu» до подключения централа.
async fn advertise<'values, 'server, C: Controller>(
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv_data = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(b"nu"),
        ],
        &mut adv_data[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv_data[..len],
                scan_data: &[],
            },
        )
        .await?;
    log::info!("BLE: advertising as \"nu\"");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    Ok(conn)
}

/// Обработка записей в характеристику: кадр → движок.
async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let frame_handle = server.nucleus.frame.handle;
    loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => {
                log::info!("gatt: disconnected ({reason:?})");
                return Ok(());
            }
            GattConnectionEvent::Gatt { event } => {
                let reply = match event {
                    GattEvent::Write(event) => {
                        if event.handle() == frame_handle {
                            // Кадр протокола от централа → движок
                            event.with_data(|_offset, data| {
                                engine::INBOX.try_send(Vec::from(data)).ok();
                            });
                        }
                        event.accept()
                    }
                    other => other.accept(),
                };
                if let Err(e) = reply.send().await {
                    log::warn!("gatt: reply error: {e:?}");
                }
            }
            _ => {}
        }
    }
}

/// Рассылка исходящих кадров подключённым централам (notify).
async fn notify_task<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    let frame_char = &server.nucleus.frame;
    loop {
        let frame = engine::OUTBOX.recv().await;
        let Ok(value) = HVec::<u8, FRAME_MAX>::from_slice(&frame) else {
            continue;
        };
        // notify без подтверждения (последний аргумент), ошибки = разрыв
        if frame_char.notify(conn, &value, false).await.is_err() {
            return;
        }
    }
}
