// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Native background owner. Its capture lock and the session lock never overlap.
//! The separate, bounded mailbox retains an in-flight batch until UI publication
//! succeeds, including resource paging. Raw evidence is independent of that path.

use super::weather_cache::WeatherCache;
use super::{
    capture::CaptureClock,
    capture_files::{CaptureArchive, CapturePolicy},
    connection::{
        Connection, ConnectionId, ConnectionPhase, Effect, Event, IoFailure, Output, WriteId,
    },
    Credentials, Product, ProtocolPhase, Weather, WeatherKind,
};
use crate::weather_sources::{StationId, StationReport, StationReportKind};
use app_ui_contracts::receiver::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, OnceLock},
};

const BATCH_SIZE: usize = 32;
static NEXT_CONSUMER: AtomicU64 = AtomicU64::new(1);

/// Pure identity allocation, called when a publisher is constructed, before any
/// asynchronous work. Attachment order must not let an older session win.
pub fn new_session_consumer() -> u64 {
    NEXT_CONSUMER.fetch_add(1, Ordering::Relaxed)
}

pub use super::SessionDelivery as Delivery;

struct Mailbox {
    next_id: u64,
    consumer: u64,
    panel: UiReceiverPanel,
    pending: BTreeMap<(StationId, StationReportKind), StationReport>,
    in_flight: Option<Arc<Delivery>>,
    dirty: bool,
    live: Arc<super::live::LiveSnapshot>,
}

impl Mailbox {
    fn publish(&mut self, panel: UiReceiverPanel, live: Arc<super::live::LiveSnapshot>) {
        if !Arc::ptr_eq(&self.live, &live) {
            self.live = live;
            self.dirty = true;
        }
        if self.panel != panel {
            self.panel = panel;
            self.dirty = true;
        }
    }

    fn queue(&mut self, reports: impl IntoIterator<Item = StationReport>) {
        // Only already-committed changes enter here, in the owner's commit order.
        // The mailbox never makes a second report-freshness decision.
        for report in reports {
            self.pending
                .insert((report.station.clone(), report.kind()), report);
        }
    }
    fn attach(
        &mut self,
        consumer: u64,
        reports: impl IntoIterator<Item = StationReport>,
    ) -> Result<(), &'static str> {
        if consumer == 0 || consumer < self.consumer {
            return Err("Stale receiver consumer");
        }
        if consumer == self.consumer {
            return Ok(());
        }
        self.consumer = consumer;
        self.in_flight = None;
        self.pending.clear();
        self.queue(reports);
        self.dirty = true;
        Ok(())
    }
    fn begin_for(&mut self, consumer: u64) -> Option<Arc<Delivery>> {
        if consumer != self.consumer {
            return None;
        }
        self.begin()
    }
    fn begin(&mut self) -> Option<Arc<Delivery>> {
        if self.in_flight.is_none() && (self.dirty || !self.pending.is_empty()) {
            self.in_flight = Some(Arc::new(Delivery {
                id: self.next_id,
                panel: self.panel.clone(),
                live: self.live.clone(),
                reports: (0..BATCH_SIZE)
                    .filter_map(|_| self.pending.pop_first().map(|(_, report)| report))
                    .collect(),
            }));
            self.next_id = self
                .next_id
                .checked_add(1)
                .expect("receiver delivery IDs exhausted");
            self.dirty = false;
        }
        self.in_flight.clone()
    }
    fn acknowledge(&mut self, delivery: &Arc<Delivery>) {
        if self
            .in_flight
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, delivery))
        {
            self.in_flight = None;
        }
    }
}

pub struct Runtime {
    root: PathBuf,
    connection: Option<Connection<CaptureArchive>>,
    devices: Vec<ReceiverDevice>,
    inventory_generation: u64,
    permitted: bool,
    error: Option<String>,
    host_failed: bool,
    credentials_ready: bool,
    import_generation: u64,
    pending_import: Option<(String, &'static str)>,
    normalized: u64,
    rejected: u64,
    weather_cache: Option<WeatherCache>,
    weather_error: Option<String>,
    last_delivery_signal_ms: Option<u64>,
    live: super::live::LiveInput,
    mailbox: Arc<Mutex<Mailbox>>,
}

impl Runtime {
    pub fn new(root: PathBuf) -> Self {
        let cache = WeatherCache::open(&root);
        let (weather_cache, weather_error) = match cache {
            Ok(cache) => (Some(cache), None),
            Err(error) => (
                None,
                Some(format!("Receiver weather cache unavailable: {error}")),
            ),
        };
        Self {
            root,
            connection: None,
            devices: Vec::new(),
            inventory_generation: 0,
            permitted: false,
            error: None,
            host_failed: false,
            credentials_ready: false,
            import_generation: 0,
            pending_import: None,
            normalized: 0,
            rejected: 0,
            weather_cache,
            weather_error,
            last_delivery_signal_ms: None,
            live: super::live::LiveInput::default(),
            mailbox: Arc::new(Mutex::new(Mailbox {
                next_id: 1,
                consumer: 0,
                panel: empty_panel(),
                pending: BTreeMap::new(),
                in_flight: None,
                dirty: true,
                live: Arc::default(),
            })),
        }
    }

    fn attach(&mut self, consumer: u64) -> Result<(), &'static str> {
        self.mailbox.lock().unwrap().attach(
            consumer,
            self.weather_cache
                .iter()
                .flat_map(|cache| cache.reports())
                .cloned(),
        )
    }

    fn ingest_weather(&mut self, reports: Vec<StationReport>) {
        if reports.is_empty() || self.weather_error.is_some() {
            return;
        }
        let Some(cache) = &mut self.weather_cache else {
            return;
        };
        match cache.ingest(reports) {
            Ok(changed) => {
                self.normalized += changed.len() as u64;
                self.mailbox.lock().unwrap().queue(changed);
            }
            Err(error) => {
                // Raw capture remains independent. Latch this fault rather than
                // claiming new observations are safe offline or retrying every RX.
                self.weather_error = Some(format!("Receiver weather cache failed: {error}. New weather is not displayed; raw recording continues. Restart the app after correcting storage."));
            }
        }
    }

    fn credentials(&self) -> Result<Credentials, String> {
        let read = |name: &str| -> Result<Vec<u8>, String> {
            let file = fs::File::open(self.root.join(name))
                .map_err(|_| "Private receiver credentials not installed")?;
            let mut bytes = Vec::new();
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read receiver credentials")?;
            Ok(bytes)
        };
        Credentials::new(&read("auth-token.bin")?, &read("auth-user.bin")?)
            .map_err(|e| e.to_string())
    }

    pub fn update(
        &mut self,
        event: ReceiverHostEvent,
        bytes: &[u8],
        clock: CaptureClock,
    ) -> ReceiverHostOutput {
        let mut effects = Vec::new();
        let mut output = Output::default();
        let event = if self.host_failed {
            ReceiverHostEvent::Tick
        } else {
            event
        };
        match event {
            ReceiverHostEvent::HostFailed => {
                if let Some(connection) = &mut self.connection {
                    output = connection.stop(clock);
                }
                self.connection = None;
                self.host_failed = true;
                self.pending_import = None;
                self.error = Some("Receiver interface failed; recording stopped. Restart the application before reconnecting.".into());
            }
            ReceiverHostEvent::FileRead {
                request_id,
                succeeded,
            } => {
                if self
                    .pending_import
                    .as_ref()
                    .is_some_and(|(id, _)| id == &request_id)
                {
                    let (_, name) = self.pending_import.take().unwrap();
                    if succeeded && !self.active() {
                        self.error = self.install_credentials_file(name, bytes).err();
                        self.credentials_ready = self.credentials().is_ok();
                    } else if !succeeded {
                        self.error = Some("Receiver credential file was not read; select a local file and try again".into());
                    }
                }
            }
            ReceiverHostEvent::HostUnavailable => {
                if let Some(connection) = &mut self.connection {
                    output = connection.stop(clock);
                }
                self.connection = None;
                self.error = Some("Android stopped receiver background service; reconnect when the app is visible".into());
            }
            ReceiverHostEvent::Inventory {
                permitted,
                mut devices,
            } => {
                devices.truncate(128);
                devices.retain(|device| device.address.len() <= 64 && device.name.len() <= 256);
                devices.sort_by(|a, b| (&a.name, &a.address).cmp(&(&b.name, &b.address)));
                devices.dedup_by(|a, b| a.address == b.address);
                self.inventory_generation = self
                    .inventory_generation
                    .checked_add(1)
                    .expect("receiver inventory generation exhausted");
                self.devices = devices;
                self.permitted = permitted;
                self.credentials_ready = self.credentials().is_ok();
            }
            ReceiverHostEvent::Action { action_id } => {
                self.error = None;
                if action_id == "receiver:permission" {
                    effects.push(ReceiverHostEffect::RequestPermission);
                } else if action_id == "receiver:refresh" {
                    effects.push(ReceiverHostEffect::RefreshInventory);
                } else if matches!(
                    action_id.as_str(),
                    "receiver:import-token" | "receiver:import-bundle"
                ) && !self.active()
                {
                    self.import_generation = self
                        .import_generation
                        .checked_add(1)
                        .expect("receiver import generation exhausted");
                    let request_id = format!("receiver-import:{}", self.import_generation);
                    let name = if action_id == "receiver:import-token" {
                        "auth-token.bin"
                    } else {
                        "auth-user.bin"
                    };
                    self.pending_import = Some((request_id.clone(), name));
                    effects.push(ReceiverHostEffect::ReadPrivateFile {
                        request_id,
                        max_bytes: 4096,
                    });
                } else if action_id == "receiver:stop" {
                    if let Some(connection) = &mut self.connection {
                        output = connection.stop(clock);
                    }
                    // Drop releases the archive lock; reconnect opens a fresh recorder.
                    self.connection = None;
                } else if action_id == "receiver:export" && !self.active() {
                    self.connection = None;
                    match self.export_capture() {
                        Ok(path) => effects.push(ReceiverHostEffect::ShareFile {
                            path: path.to_string_lossy().into_owned(),
                            mime_type: "application/zip".into(),
                            title: "Share private receiver capture".into(),
                        }),
                        Err(error) => self.error = Some(error),
                    }
                } else {
                    let device = self
                        .devices
                        .iter()
                        .enumerate()
                        .find(|(index, _)| action_id == self.device_action(*index))
                        .map(|(_, device)| device.address.clone());
                    if let Some(device) = device.filter(|_| self.permitted && !self.active()) {
                        // Blocked owners must release the file lock before opening a new archive.
                        self.connection = None;
                        match self.credentials().and_then(|credentials| {
                            CaptureArchive::open(
                                &self.root.join("captures"),
                                CapturePolicy::default(),
                            )
                            .map(|archive| Connection::new(credentials, archive, clock))
                            .map_err(|e| format!("Cannot open private receiver capture: {e}"))
                        }) {
                            Ok(mut connection) => {
                                output = connection.start(device, clock);
                                self.connection = Some(connection);
                            }
                            Err(error) => self.error = Some(error),
                        }
                    } else {
                        self.error = Some(
                            "Receiver selection changed; refresh paired devices and try again"
                                .into(),
                        );
                    }
                }
            }
            event => {
                if let Some(connection) = &mut self.connection {
                    let event = match event {
                        ReceiverHostEvent::Connected { connection_id } => {
                            Event::Connected(ConnectionId(connection_id))
                        }
                        ReceiverHostEvent::Received { connection_id } => {
                            Event::Received(ConnectionId(connection_id), bytes)
                        }
                        ReceiverHostEvent::Written {
                            connection_id,
                            write_id,
                        } => Event::Written(ConnectionId(connection_id), WriteId(write_id)),
                        ReceiverHostEvent::Failed {
                            connection_id,
                            failure,
                        } => Event::Failed(
                            ConnectionId(connection_id),
                            match failure {
                                ReceiverIoFailure::Connection => IoFailure::Connection,
                                ReceiverIoFailure::Read => IoFailure::Read,
                                ReceiverIoFailure::Write => IoFailure::Write,
                                ReceiverIoFailure::Permission => IoFailure::Permission,
                                ReceiverIoFailure::BluetoothDisabled => {
                                    IoFailure::BluetoothDisabled
                                }
                            },
                        ),
                        ReceiverHostEvent::Tick => Event::Tick,
                        _ => unreachable!(),
                    };
                    output = connection.update(event, clock);
                } else if let ReceiverHostEvent::Connected { connection_id } = event {
                    effects.push(ReceiverHostEffect::Close { connection_id });
                }
            }
        }
        for effect in output.effects {
            effects.push(match effect {
                Effect::Connect {
                    id,
                    device,
                    service_uuid,
                } => ReceiverHostEffect::Connect {
                    connection_id: id.0,
                    device,
                    service_uuid: service_uuid.into(),
                },
                Effect::Close { id } => ReceiverHostEffect::Close {
                    connection_id: id.0,
                },
                Effect::Write {
                    id,
                    write_id,
                    bytes,
                } => ReceiverHostEffect::Write {
                    connection_id: id.0,
                    write_id: write_id.0,
                    bytes_base64: STANDARD.encode(bytes),
                },
            });
        }
        self.live
            .set_connected(self.connection.as_ref().is_some_and(|connection| {
                connection.status().phase == ConnectionPhase::Protocol(ProtocolPhase::Receiving)
            }));
        for message in output.messages {
            if let Ok(Product::Traffic(traffic)) = &message.product {
                self.live.ingest(traffic, clock);
            }
            if let Ok(Product::Nmea { sentence }) = &message.product {
                self.live.ingest_nmea(sentence, clock);
            }
            if let Ok(Product::Weather(Weather::Radar(radar))) = &message.product {
                if self.live.ingest_radar(radar, clock).is_err() {
                    self.rejected += 1;
                }
            }
            if let Ok(Product::Weather(Weather::Text { kind, reports, .. })) = message.product {
                let report_kind = match kind {
                    WeatherKind::Metar => StationReportKind::Metar,
                    WeatherKind::Taf => StationReportKind::Taf,
                    _ => continue,
                };
                let mut normalized = Vec::new();
                for report in reports {
                    let parsed = DateTime::<Utc>::from_timestamp_millis(clock.wall_epoch_ms)
                        .ok_or("Invalid receiver wall clock")
                        .and_then(|now| StationReport::from_receiver(report_kind, &report, now));
                    match parsed {
                        Ok(report) => normalized.push(report),
                        Err(_) => self.rejected += 1,
                    }
                }
                self.ingest_weather(normalized);
            }
        }
        self.live.expire(clock);
        let panel = self.panel();
        self.mailbox
            .lock()
            .unwrap()
            .publish(panel.clone(), self.live.snapshot());
        let pending = {
            let mailbox = self.mailbox.lock().unwrap();
            mailbox.dirty || !mailbox.pending.is_empty() || mailbox.in_flight.is_some()
        };
        let delivery_ready = pending
            && self
                .last_delivery_signal_ms
                .is_none_or(|last| clock.monotonic_ms.saturating_sub(last) >= 1000);
        if delivery_ready {
            self.last_delivery_signal_ms = Some(clock.monotonic_ms);
        }
        ReceiverHostOutput {
            delivery_ready,
            effects,
            keep_alive: self.active(),
            panel,
            next_wake_monotonic_ms: self
                .connection
                .as_ref()
                .and_then(Connection::next_wake_ms)
                .into_iter()
                .chain(pending.then_some(clock.monotonic_ms.saturating_add(1000)))
                .chain(self.live.next_wake_ms())
                .min(),
        }
    }

    fn active(&self) -> bool {
        self.connection.as_ref().is_some_and(|connection| {
            !matches!(
                connection.status().phase,
                ConnectionPhase::Stopped | ConnectionPhase::Blocked
            )
        })
    }

    fn install_credentials_file(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        if (name == "auth-token.bin" && bytes.len() != 48)
            || (name == "auth-user.bin" && (!bytes.starts_with(b"GRM_SALT") || bytes.len() > 4096))
        {
            return Err("Not a valid receiver credential file".into());
        }
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            let mut directory = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                directory.mode(0o700);
            }
            match directory.create(&self.root) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            let temporary = self.root.join(format!("{name}.pending"));
            let mut options = fs::OpenOptions::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(temporary, self.root.join(name))?;
            fs::File::open(&self.root)?.sync_all()
        };
        write().map_err(|_| "Cannot save private receiver credentials".into())
    }
    fn device_action(&self, index: usize) -> String {
        format!("receiver:device:{}:{index}", self.inventory_generation)
    }

    fn export_capture(&self) -> Result<PathBuf, String> {
        let export = || -> Result<PathBuf, Box<dyn std::error::Error>> {
            let mut archive =
                CaptureArchive::open(&self.root.join("captures"), CapturePolicy::default())?;
            let paths = archive.seal_for_export()?;
            if paths.is_empty() {
                return Err("No receiver captures are available".into());
            }
            let destination = self.root.join("exports");
            let mut directory = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                directory.mode(0o700);
            }
            match directory.create(&destination) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            let temporary = destination.join("receiver-capture.pending");
            let mut options = fs::OpenOptions::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for path in paths {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Invalid capture filename")?;
                zip.start_file(name, options)?;
                std::io::copy(&mut fs::File::open(path)?, &mut zip)?;
            }
            zip.finish()?.sync_all()?;
            let path = destination.join("receiver-capture.zip");
            fs::rename(temporary, &path)?;
            fs::File::open(destination)?.sync_all()?;
            Ok(path)
        };
        export().map_err(|error| format!("Cannot export receiver capture: {error}"))
    }
    fn panel(&self) -> UiReceiverPanel {
        let status = self.connection.as_ref().map(Connection::status);
        let mut panel = empty_panel();
        panel.status = if let Some(error) = &self.error {
            error.clone()
        } else {
            match status.as_ref().map(|status| status.phase) {
                None | Some(ConnectionPhase::Stopped) => "Disconnected".into(),
                Some(ConnectionPhase::Connecting) => "Connecting".into(),
                Some(ConnectionPhase::Protocol(ProtocolPhase::Receiving)) => {
                    "Receiving and recording".into()
                }
                Some(ConnectionPhase::Protocol(_)) => "Authenticating receiver".into(),
                Some(ConnectionPhase::WaitingToRetry { .. }) => {
                    "Connection interrupted; retrying automatically".into()
                }
                Some(ConnectionPhase::Blocked) => "Receiver stopped".into(),
            }
        };
        if let Some(status) = status {
            panel.detail = format!("{} MiB recorded; {} messages decoded; {} malformed. {} weather reports accepted; {} rejected.{}",
                status.captured.written_bytes / (1024*1024),status.decoded_messages,status.malformed_messages,
                self.normalized,self.rejected,
                status.last_fault.map(|fault| format!(" {}",fault.description())).unwrap_or_default());
        }
        if let Some(error) = &self.weather_error {
            panel.detail.push_str(&format!(" {error}"));
        }
        let live = self.live.snapshot();
        if live.connected {
            panel.detail.push_str(&format!(
                " {} traffic records. Receiver GPS: {}. Enable ADS-B Traffic in Layers; choose Receiver in the ownship menu. Use BARO and NEXRAD grid trays to select receiver sources.",
                live.aircraft.len(), if live.ownship.is_some() { "fix available" } else { "no current fix" },
            ));
        }
        let action = |id: &str, label: &str| UiReceiverAction {
            action_id: id.into(),
            label: label.into(),
            enabled: true,
            disabled_reason: None,
        };
        if self.host_failed {
            return panel;
        }
        if self.active() {
            panel
                .actions
                .push(action("receiver:stop", "Disconnect and stop recording"));
        } else {
            panel
                .actions
                .push(action("receiver:export", "Export private capture"));
            if !self.credentials_ready {
                panel.detail.push_str(" Import the private auth-token.bin and auth-user.bin files before connecting. They are kept only on this device, never cloud-synced.");
            }
            panel
                .actions
                .push(action("receiver:import-token", "Import receiver token"));
            panel
                .actions
                .push(action("receiver:import-bundle", "Import receiver bundle"));
            if !self.permitted {
                panel
                    .actions
                    .push(action("receiver:permission", "Allow Bluetooth"));
                return panel;
            }
            panel
                .actions
                .push(action("receiver:refresh", "Refresh paired devices"));
            panel
                .actions
                .extend(self.devices.iter().enumerate().map(|(index, device)| {
                    let mut button = action(
                        &self.device_action(index),
                        &format!("Connect {} ({})", device.name, device.address),
                    );
                    button.enabled = self.credentials_ready;
                    button.disabled_reason = (!self.credentials_ready)
                        .then(|| "Import both private receiver credential files first".into());
                    button
                }));
        }
        panel
    }
}

fn empty_panel() -> UiReceiverPanel {
    UiReceiverPanel { title:"ADS-B receiver (experimental)".into(),status:"Disconnected".into(),
        detail:"Pair the GTX 345 in Android Bluetooth settings first. Connecting automatically records a private capture (256 MiB limit). Select Receiver in the ownship menu for GPS; BARO/NEXRAD grid trays choose pressure and radar sources. Radar placement is experimental; AHRS is recorded but not displayed.".into(),actions:Vec::new() }
}

// Single process owner survives activity rotation and UI session replacement.
struct Owner {
    root: PathBuf,
    runtime: Mutex<Runtime>,
    mailbox: Arc<Mutex<Mailbox>>,
}
static OWNER: OnceLock<Owner> = OnceLock::new();

pub fn initialize(root: &Path) -> Result<(), String> {
    if let Some(owner) = OWNER.get() {
        return if owner.root == root {
            Ok(())
        } else {
            Err("Receiver owner already initialized for another private directory".into())
        };
    }
    let runtime = Runtime::new(root.to_owned());
    let _ = OWNER.set(Owner {
        root: root.to_owned(),
        mailbox: runtime.mailbox.clone(),
        runtime: Mutex::new(runtime),
    });
    if OWNER.get().unwrap().root != root {
        return Err("Receiver owner already initialized for another private directory".into());
    }
    Ok(())
}

pub fn update(
    event: ReceiverHostEvent,
    bytes: &[u8],
    clock: CaptureClock,
) -> Result<ReceiverHostOutput, String> {
    if bytes.len() > 65535 {
        return Err("Receiver transport chunk exceeds bound".into());
    }
    let owner = OWNER.get().ok_or("Receiver owner not initialized")?;
    Ok(owner
        .runtime
        .lock()
        .map_err(|_| "Receiver owner failed")?
        .update(event, bytes, clock))
}

pub fn attach_session(consumer: u64) -> Result<(), String> {
    OWNER
        .get()
        .ok_or("Receiver owner not initialized")?
        .runtime
        .lock()
        .map_err(|_| "Receiver owner failed")?
        .attach(consumer)
        .map_err(str::to_owned)
}

pub fn begin_delivery(consumer: u64) -> Option<Arc<Delivery>> {
    OWNER.get()?.mailbox.lock().ok()?.begin_for(consumer)
}

pub fn acknowledge_delivery(delivery: &Arc<Delivery>) {
    if let Some(owner) = OWNER.get() {
        owner.mailbox.lock().unwrap().acknowledge(delivery);
    }
}

pub fn delivery(id: u64) -> Result<Arc<Delivery>, String> {
    OWNER
        .get()
        .and_then(|owner| owner.mailbox.lock().ok()?.in_flight.clone())
        .filter(|delivery| delivery.id == id)
        .ok_or("Stale receiver delivery".into())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Temp;
    use super::*;

    fn clock(ms: u64) -> CaptureClock {
        CaptureClock {
            monotonic_ms: ms,
            wall_epoch_ms: 1791547200000 + ms as i64,
        }
    }
    fn action(id: &str) -> ReceiverHostEvent {
        ReceiverHostEvent::Action {
            action_id: id.into(),
        }
    }
    fn inventory() -> ReceiverHostEvent {
        ReceiverHostEvent::Inventory {
            permitted: true,
            devices: vec![ReceiverDevice {
                address: "00:00:00:00:00:01".into(),
                name: "Test receiver".into(),
            }],
        }
    }
    fn root() -> Temp {
        let root = Temp::new();
        drop(WeatherCache::open(root.path()).unwrap());
        fs::write(root.path().join("auth-token.bin"), [7; 48]).unwrap();
        fs::write(
            root.path().join("auth-user.bin"),
            b"GRM_SALT synthetic test only",
        )
        .unwrap();
        root
    }
    fn start(runtime: &mut Runtime, now: u64) -> u64 {
        let panel = runtime.update(inventory(), &[], clock(now)).panel;
        let id = panel.actions.last().unwrap().action_id.clone();
        let output = runtime.update(action(&id), &[], clock(now));
        assert!(output.keep_alive);
        match output.effects[0] {
            ReceiverHostEffect::Connect { connection_id, .. } => connection_id,
            _ => panic!("no connection"),
        }
    }
    fn report(minute: u8) -> StationReport {
        StationReport::from_receiver(
            StationReportKind::Metar,
            &super::super::TextReport {
                station: Some("KPAE".into()),
                text: format!("KPAE 0912{minute:02}Z 00000KT 10SM CLR 10/05 A3000"),
                notam_identifier: None,
                record_type_raw: None,
            },
            DateTime::from_timestamp_millis(clock(0).wall_epoch_ms).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn credential_import_is_bounded_private_validated_and_old_picker_results_are_ignored() {
        let root = Temp::new();
        let mut runtime = Runtime::new(root.path().into());
        let requested = runtime.update(action("receiver:import-token"), &[], clock(0));
        let ReceiverHostEffect::ReadPrivateFile {
            request_id: old, ..
        } = &requested.effects[0]
        else {
            panic!()
        };
        let requested = runtime.update(action("receiver:import-token"), &[], clock(1));
        let ReceiverHostEffect::ReadPrivateFile { request_id, .. } = &requested.effects[0] else {
            panic!()
        };
        runtime.update(
            ReceiverHostEvent::FileRead {
                request_id: old.clone(),
                succeeded: true,
            },
            &[7; 48],
            clock(2),
        );
        assert!(!root.path().join("auth-token.bin").exists());
        runtime.update(
            ReceiverHostEvent::FileRead {
                request_id: request_id.clone(),
                succeeded: true,
            },
            &[7; 48],
            clock(3),
        );
        assert_eq!(
            fs::read(root.path().join("auth-token.bin")).unwrap(),
            vec![7; 48]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.path().join("auth-token.bin"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let requested = runtime.update(action("receiver:import-bundle"), &[], clock(4));
        let ReceiverHostEffect::ReadPrivateFile { request_id, .. } = &requested.effects[0] else {
            panic!()
        };
        runtime.update(
            ReceiverHostEvent::FileRead {
                request_id: request_id.clone(),
                succeeded: true,
            },
            &[0; 4097],
            clock(5),
        );
        assert!(!root.path().join("auth-user.bin").exists());
        assert!(!runtime.credentials_ready);
        let requested = runtime.update(action("receiver:import-bundle"), &[], clock(6));
        let ReceiverHostEffect::ReadPrivateFile { request_id, .. } = &requested.effects[0] else {
            panic!()
        };
        let result = runtime.update(
            ReceiverHostEvent::FileRead {
                request_id: request_id.clone(),
                succeeded: true,
            },
            b"GRM_SALT synthetic",
            clock(7),
        );
        assert!(runtime.credentials_ready);
        assert!(
            !result.keep_alive,
            "import never initiates an unsolicited connection"
        );
    }

    #[test]
    fn ui_selection_is_core_owned_permission_gated_and_stale_inventory_cannot_select_another_device(
    ) {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        let initial = runtime.update(ReceiverHostEvent::Tick, &[], clock(0));
        let permission = initial
            .panel
            .actions
            .iter()
            .find(|action| action.label == "Allow Bluetooth")
            .unwrap();
        assert!(matches!(
            runtime
                .update(action(&permission.action_id), &[], clock(1))
                .effects[0],
            ReceiverHostEffect::RequestPermission
        ));
        let selection = runtime
            .update(inventory(), &[], clock(2))
            .panel
            .actions
            .last()
            .unwrap()
            .action_id
            .clone();
        runtime.update(inventory(), &[], clock(3));
        let stale = runtime.update(action(&selection), &[], clock(4));
        assert!(!stale.keep_alive);
        assert!(stale.panel.status.contains("selection changed"));
        assert!(stale.effects.is_empty());
    }

    #[test]
    fn real_private_capture_outlives_ui_delivery_and_old_socket_cannot_restart_it() {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        let id = start(&mut runtime, 0);
        let delivery = runtime.mailbox.lock().unwrap().begin().unwrap();
        // Simulate a session publication held indefinitely in resource paging.
        let connected = runtime.update(
            ReceiverHostEvent::Connected { connection_id: id },
            &[],
            clock(1),
        );
        assert!(connected
            .effects
            .iter()
            .any(|effect| matches!(effect, ReceiverHostEffect::Write { .. })));
        runtime.update(
            ReceiverHostEvent::Received { connection_id: id },
            &[0x7e],
            clock(2),
        );
        let stopped = runtime.update(action("receiver:stop"), &[], clock(3));
        assert!(!stopped.keep_alive);
        assert!(fs::read_dir(root.path().join("captures"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|e| e == "abrc")));
        let late = runtime.update(
            ReceiverHostEvent::Connected { connection_id: id },
            &[],
            clock(4),
        );
        assert!(
            matches!(late.effects[0],ReceiverHostEffect::Close {connection_id} if connection_id == id)
        );
        assert!(Arc::ptr_eq(
            &delivery,
            &runtime.mailbox.lock().unwrap().begin().unwrap()
        ));
        runtime.mailbox.lock().unwrap().acknowledge(&delivery);
        assert_eq!(
            runtime
                .mailbox
                .lock()
                .unwrap()
                .begin()
                .unwrap()
                .panel
                .status,
            "Disconnected"
        );
        let exported = runtime.update(action("receiver:export"), &[], clock(5));
        let ReceiverHostEffect::ShareFile { path, .. } = &exported.effects[0] else {
            panic!("missing export")
        };
        let replay =
            super::super::capture_replay::replay_export(fs::File::open(path).unwrap(), |_, _| {})
                .unwrap();
        assert_eq!(replay.connections, 1);
        assert_eq!(replay.received_bytes, 1);
        assert_eq!(replay.incomplete_connections, 1);
        assert_eq!(replay.torn_tails, 0);
        let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
        assert!(!zip.is_empty());
        for index in 0..zip.len() {
            let mut file = zip.by_index(index).unwrap();
            assert!(
                file.name().ends_with(".abrc"),
                "do not export credential source files"
            );
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            assert_eq!(
                bytes,
                fs::read(root.path().join("captures").join(file.name())).unwrap()
            );
        }
    }

    #[test]
    fn mailbox_coalesces_by_report_time_and_ack_does_not_consume_newer_arrivals() {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        runtime.update(ReceiverHostEvent::Tick, &[], clock(0));
        runtime.ingest_weather(vec![report(10)]);
        let a = runtime.mailbox.lock().unwrap().begin().unwrap();
        assert_eq!(a.reports.len(), 1);
        runtime.ingest_weather(vec![report(12), report(11)]);
        let mut mailbox = runtime.mailbox.lock().unwrap();
        assert!(
            Arc::ptr_eq(&a, &mailbox.begin().unwrap()),
            "failure retries exactly the same delivery"
        );
        mailbox.acknowledge(&a);
        let b = mailbox.begin().unwrap();
        assert_eq!(b.reports[0].raw_text, report(12).raw_text);
        mailbox.acknowledge(&a);
        assert!(
            Arc::ptr_eq(&b, &mailbox.begin().unwrap()),
            "old completion cannot consume next batch"
        );
        mailbox.acknowledge(&b);
        assert!(mailbox.begin().is_none());
    }

    #[test]
    fn denied_background_service_stops_connection_instead_of_reconnecting_without_capture_owner() {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        let id = start(&mut runtime, 0);
        let stopped = runtime.update(ReceiverHostEvent::HostUnavailable, &[], clock(1));
        assert!(!stopped.keep_alive);
        assert!(
            matches!(stopped.effects[0],ReceiverHostEffect::Close {connection_id} if connection_id==id)
        );
        assert!(stopped.panel.status.contains("Android stopped"));
        assert!(runtime
            .update(ReceiverHostEvent::Tick, &[], clock(100_000))
            .effects
            .is_empty());
    }

    #[test]
    fn bounded_delivery_and_core_throttle_do_not_lose_disconnected_status() {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        assert!(
            runtime
                .update(ReceiverHostEvent::Tick, &[], clock(0))
                .delivery_ready
        );
        let initial = runtime.mailbox.lock().unwrap().begin().unwrap();
        runtime.mailbox.lock().unwrap().acknowledge(&initial);
        let update = runtime.update(inventory(), &[], clock(10));
        assert!(!update.delivery_ready);
        assert!(
            update.next_wake_monotonic_ms.is_some(),
            "must wake even without an active socket"
        );
        assert!(
            runtime
                .update(ReceiverHostEvent::Tick, &[], clock(1010))
                .delivery_ready
        );
        let mut mailbox = runtime.mailbox.lock().unwrap();
        for i in 0..100 {
            let mut record = report(0);
            record.station = crate::weather_sources::StationId::new(&format!("X{i:03}")).unwrap();
            mailbox.queue([record]);
        }
        let mut count = 0;
        while let Some(delivery) = mailbox.begin() {
            assert!(delivery.reports.len() <= BATCH_SIZE);
            count += delivery.reports.len();
            mailbox.acknowledge(&delivery);
        }
        assert_eq!(count, 100);
    }

    #[test]
    fn live_mailbox_retains_one_latest_snapshot_while_weather_pages_and_old_leases_retry() {
        let root = root();
        let runtime = Runtime::new(root.path().into());
        let mut input = super::super::live::LiveInput::default();
        input.set_connected(true);
        input.ingest(
            &super::super::test_support::traffic(clock(0).wall_epoch_ms),
            clock(0),
        );
        let mut mailbox = runtime.mailbox.lock().unwrap();
        let consumer = new_session_consumer();
        mailbox.attach(consumer, []).unwrap();
        mailbox.publish(empty_panel(), input.snapshot());
        for index in 0..100 {
            let mut report = report(0);
            report.station = StationId::new(&format!("X{index:03}")).unwrap();
            mailbox.queue([report]);
        }
        let first = mailbox.begin_for(consumer).unwrap();
        assert_eq!(first.reports.len(), BATCH_SIZE);
        for now in [1000, 2000, 3000] {
            input.ingest(
                &super::super::test_support::traffic(clock(now).wall_epoch_ms),
                clock(now),
            );
            mailbox.publish(empty_panel(), input.snapshot());
        }
        assert!(Arc::ptr_eq(&first, &mailbox.begin_for(consumer).unwrap()));
        assert_eq!(
            first.live.ownship_at(clock(0)).unwrap().event_time_epoch_ms,
            clock(0).wall_epoch_ms
        );
        mailbox.acknowledge(&first);
        let second = mailbox.begin_for(consumer).unwrap();
        assert_eq!(second.reports.len(), BATCH_SIZE);
        assert_eq!(
            second
                .live
                .ownship_at(clock(3000))
                .unwrap()
                .event_time_epoch_ms,
            clock(3000).wall_epoch_ms
        );
        mailbox.acknowledge(&first);
        assert!(Arc::ptr_eq(&second, &mailbox.begin_for(consumer).unwrap()));
        let next = new_session_consumer();
        mailbox.attach(next, []).unwrap();
        let reattached = mailbox.begin_for(next).unwrap();
        assert!(Arc::ptr_eq(&reattached.live, &second.live));
        assert!(reattached.live.ownship_at(clock(13_000)).is_none());
        // Even when the wall clock stops, a stalled UI cannot revive a lease.
        assert!(reattached
            .live
            .ownship_at(CaptureClock {
                monotonic_ms: 13_000,
                wall_epoch_ms: clock(3000).wall_epoch_ms,
            })
            .is_none());
    }

    #[test]
    fn replacement_session_restores_acknowledged_weather_and_rejects_old_consumers() {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        runtime.update(inventory(), &[], clock(0));
        let old = new_session_consumer();
        runtime.attach(old).unwrap();
        runtime.ingest_weather(vec![report(10)]);
        let a = runtime.mailbox.lock().unwrap().begin_for(old).unwrap();
        runtime.mailbox.lock().unwrap().acknowledge(&a);
        assert!(runtime.mailbox.lock().unwrap().begin_for(old).is_none());
        let new = new_session_consumer();
        runtime.attach(new).unwrap();
        let b = runtime.mailbox.lock().unwrap().begin_for(new).unwrap();
        assert_eq!(b.reports, [report(10)]);
        assert_eq!(b.panel, a.panel);
        assert!(runtime.mailbox.lock().unwrap().begin_for(old).is_none());
        assert!(
            runtime.attach(old).is_err(),
            "late old publisher cannot reclaim the stream"
        );
        runtime.attach(new).unwrap();
        runtime.mailbox.lock().unwrap().acknowledge(&a);
        assert!(Arc::ptr_eq(
            &b,
            &runtime.mailbox.lock().unwrap().begin_for(new).unwrap()
        ));
        runtime.ingest_weather(vec![report(12)]);
        runtime.mailbox.lock().unwrap().acknowledge(&b);
        let c = runtime.mailbox.lock().unwrap().begin_for(new).unwrap();
        assert_eq!(c.reports, [report(12)]);
        runtime.mailbox.lock().unwrap().acknowledge(&c);
        drop(runtime);
        let mut restarted = Runtime::new(root.path().into());
        restarted.update(ReceiverHostEvent::Tick, &[], clock(999_000));
        let session = new_session_consumer();
        restarted.attach(session).unwrap();
        let restored = restarted
            .mailbox
            .lock()
            .unwrap()
            .begin_for(session)
            .unwrap();
        assert_eq!(restored.reports, [report(12)]);
        assert_eq!(restored.panel.status, "Disconnected");
        assert!(
            restarted.connection.is_none(),
            "restoring weather must not connect or restore live sensors"
        );
    }

    #[test]
    fn offline_restart_and_session_replacement_restore_real_flight_plan_weather_actions() {
        use crate::{
            session, FlightPlan, FlightPlanRowActionEffect, FlightPlanRowActionId,
            HadOperationOutcome, NavRef, RouteComponent,
        };
        let root = root();
        let mut recording = Runtime::new(root.path().into());
        recording.ingest_weather(vec![report(12)]);
        drop(recording);
        let mut runtime = Runtime::new(root.path().into());
        runtime.update(ReceiverHostEvent::Tick, &[], clock(14 * 60_000));
        // Both sessions start without network, Internet weather, or a receiver.
        for _ in 0..2 {
            let init = session::create_ui_session_at_epoch_ms(
                FlightPlan {
                    route_components: vec![RouteComponent::Waypoint {
                        waypoint: NavRef::Airport("KPAE".into()),
                    }],
                    route_component_uids: vec!["airport-row".into()],
                    route_component_uid_counter: 1,
                    aircraft: None,
                    ..FlightPlan::default()
                },
                &[],
                None,
                None,
                clock(14 * 60_000).wall_epoch_ms,
            )
            .unwrap();
            let consumer = new_session_consumer();
            runtime.attach(consumer).unwrap();
            let delivery = runtime.mailbox.lock().unwrap().begin_for(consumer).unwrap();
            assert!(matches!(
                session::apply_receiver_delivery_in_session(
                    init.handle,
                    &delivery,
                    clock(14 * 60_000)
                )
                .unwrap(),
                HadOperationOutcome::Complete { .. }
            ));
            runtime.mailbox.lock().unwrap().acknowledge(&delivery);
            let HadOperationOutcome::Complete { result, .. } =
                session::get_session_snapshot(init.handle).unwrap()
            else {
                panic!("offline weather must not request a network resource");
            };
            let snapshot: session::UiSessionSnapshot = serde_json::from_value(result).unwrap();
            assert!(snapshot.settings_page_state.blocks.iter().any(|block| matches!(block,
                app_ui_contracts::session::UiSettingsPageBlock::Receiver { panel } if panel.status == "Disconnected")));
            let plan = snapshot.app_ui_state.active_plan.unwrap();
            let row = plan
                .display_rows
                .iter()
                .find(|row| row.nav_ref == Some(NavRef::Airport("KPAE".into())))
                .unwrap();
            let action = crate::planning::flight_plan_row_actions(row)
                .find(|action| action.id == FlightPlanRowActionId::Weather)
                .unwrap();
            assert!(action.enabled);
            let decision = session::flight_plan_row_action_decision_in_session(
                init.handle,
                row.uid.clone(),
                action.uid.clone(),
            )
            .unwrap();
            let Some(FlightPlanRowActionEffect::ShowWeather { detail }) = decision.effect else {
                panic!("WX modal is unavailable");
            };
            assert_eq!(
                detail.metar_text.as_deref(),
                Some(report(12).raw_text.as_str())
            );
            assert_eq!(detail.metar_age_label.as_deref(), Some("2m old"));
            session::destroy_session(init.handle);
        }
    }

    #[test]
    fn broken_weather_cache_is_visible_and_does_not_stop_raw_capture() {
        let root = root();
        fs::write(root.path().join("weather.sqlite"), b"broken database").unwrap();
        let mut runtime = Runtime::new(root.path().into());
        let id = start(&mut runtime, 0);
        let output = runtime.update(
            ReceiverHostEvent::Connected { connection_id: id },
            &[],
            clock(1),
        );
        assert!(output.keep_alive);
        assert!(output.panel.detail.contains("weather cache unavailable"));
        runtime.ingest_weather(vec![report(10)]);
        let delivery = runtime.mailbox.lock().unwrap().begin().unwrap();
        assert!(
            delivery.reports.is_empty(),
            "cannot claim undurable weather"
        );
        runtime.update(action("receiver:stop"), &[], clock(2));
        assert!(
            runtime.export_capture().is_ok(),
            "raw evidence remains available"
        );
        assert_eq!(
            fs::read(root.path().join("weather.sqlite")).unwrap(),
            b"broken database"
        );
    }

    #[test]
    fn failed_host_has_no_live_controls_and_cannot_be_restarted_by_a_queued_action() {
        let root = root();
        let mut runtime = Runtime::new(root.path().into());
        let panel = runtime.update(inventory(), &[], clock(0)).panel;
        let start = panel.actions.last().unwrap().action_id.clone();
        let failed = runtime.update(ReceiverHostEvent::HostFailed, &[], clock(1));
        assert!(failed.panel.actions.is_empty());
        assert!(failed.panel.status.contains("Restart"));
        let late = runtime.update(action(&start), &[], clock(2));
        assert!(!late.keep_alive);
        assert!(late.effects.is_empty());
        assert_eq!(failed.panel, late.panel);
    }
}
