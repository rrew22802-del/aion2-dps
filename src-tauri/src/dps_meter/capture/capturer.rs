#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_uint, CStr, CString};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, RwLock,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use libloading::{Library, Symbol};
use serde::Serialize;

use crate::dps_meter::capture::channel::Channel;
use crate::plugins::logger::AppLogger;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedPacket {
    pub src_ip: [u8; 4],
    pub src_port: u16,
    pub dst_ip: [u8; 4],
    pub dst_port: u16,
    pub sequence: u32,
    pub data: Vec<u8>,
    pub captured_at: f64,
}

#[derive(Debug, Clone)]
pub struct CaptureTarget {
    pub device_name: String,
    pub target_port: Option<String>,
}

type PcapT = *mut std::ffi::c_void;
type PcapIfT = *mut PcapIf;

#[repr(C)]
struct PcapIf {
    next: *mut PcapIf,
    name: *const c_char,
    description: *const c_char,
    addresses: *mut PcapAddr,
    flags: c_uint,
}

#[repr(C)]
struct PcapAddr {
    next: *mut PcapAddr,
    addr: *mut SockAddr,
    netmask: *mut SockAddr,
    broadaddr: *mut SockAddr,
    dstaddr: *mut SockAddr,
}

#[repr(C)]
struct SockAddr {
    sa_family: u16,
    sa_data: [u8; 14],
}

#[repr(C)]
struct PcapPkthdr {
    ts_sec: i32,
    ts_usec: i32,
    caplen: c_uint,
    len: c_uint,
}

const PCAP_IF_LOOPBACK: c_uint = 0x0000_0001;
const PCAP_IF_CONNECTION_STATUS: c_uint = 0x0030;
const PCAP_IF_CONNECTION_STATUS_DISCONNECTED: c_uint = 0x0020;
const MAGIC_PATTERN: [u8; 3] = [0x0E, 0x00, 0x36];
/// A flow counts as the game only after this many packets *start* with the magic within one detection window.
/// The game puts it at offset 0 of nearly every server packet (~20/s); matching it anywhere in a payload let
/// encrypted VPN traffic (xray/booster tunnels, 1 hit per ~12k packets) be taken for the game (owner log 05.10).
const MAGIC_MIN_HITS: u32 = 3;

#[derive(Clone)]
struct DeviceInfo {
    name: String,
    description: String,
    has_addresses: bool,
    is_loopback: bool,
}

impl DeviceInfo {
    fn label(&self) -> &str {
        if self.description.is_empty() {
            &self.name
        } else {
            &self.description
        }
    }

    fn is_virtual(&self) -> bool {
        let label = self.label().to_ascii_lowercase();
        let name = self.name.to_ascii_lowercase();

        self.is_loopback
            || name.contains("loopback")
            || label.contains("loopback")
            || ["tunnel", "happ", "xray", "sing", "v2ray", "clash", "wintun", "wireguard", "tap", "openvpn", "outline", "exitlag", "gearup", "noping", "lagofast", "mudfish", "radmin", "zerotier", "hamachi"]
                .iter()
                .any(|needle| label.contains(needle) || name.contains(needle))
    }

    fn priority_label(&self) -> &'static str {
        if self.is_loopback { "loopback" } else if self.is_virtual() { "tunnel-or-virtual" } else { "physical" }
    }
}

struct NpcapLib {
    _lib: Library,
    findalldevs: unsafe extern "C" fn(*mut PcapIfT, *mut c_char) -> c_int,
    freealldevs: unsafe extern "C" fn(PcapIfT),
    open_live: unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, *mut c_char) -> PcapT,
    close: unsafe extern "C" fn(PcapT),
    next_ex: unsafe extern "C" fn(PcapT, *mut *mut PcapPkthdr, *mut *const u8) -> c_int,
}

impl NpcapLib {
    fn load() -> Result<Self, String> {
        let lib = unsafe {
            Library::new("wpcap.dll").map_err(|error| {
                format!(
                    "Failed to load wpcap.dll. Please install Npcap from https://npcap.com. Error: {error}"
                )
            })?
        };

        unsafe {
            let findalldevs: Symbol<unsafe extern "C" fn(*mut PcapIfT, *mut c_char) -> c_int> = lib
                .get(b"pcap_findalldevs")
                .map_err(|error| format!("pcap_findalldevs: {error}"))?;
            let freealldevs: Symbol<unsafe extern "C" fn(PcapIfT)> =
                lib.get(b"pcap_freealldevs")
                    .map_err(|error| format!("pcap_freealldevs: {error}"))?;
            let open_live: Symbol<
                unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, *mut c_char) -> PcapT,
            > = lib
                .get(b"pcap_open_live")
                .map_err(|error| format!("pcap_open_live: {error}"))?;
            let close: Symbol<unsafe extern "C" fn(PcapT)> = lib
                .get(b"pcap_close")
                .map_err(|error| format!("pcap_close: {error}"))?;
            let next_ex: Symbol<
                unsafe extern "C" fn(PcapT, *mut *mut PcapPkthdr, *mut *const u8) -> c_int,
            > = lib
                .get(b"pcap_next_ex")
                .map_err(|error| format!("pcap_next_ex: {error}"))?;

            Ok(Self {
                findalldevs: *findalldevs,
                freealldevs: *freealldevs,
                open_live: *open_live,
                close: *close,
                next_ex: *next_ex,
                _lib: lib,
            })
        }
    }

    fn find_all_devices(&self) -> Result<Vec<DeviceInfo>, String> {
        let mut all_devices: PcapIfT = ptr::null_mut();
        let mut errbuf = [0u8; 256];

        let ret =
            unsafe { (self.findalldevs)(&mut all_devices, errbuf.as_mut_ptr() as *mut c_char) };
        if ret != 0 || all_devices.is_null() {
            let error = unsafe { CStr::from_ptr(errbuf.as_ptr() as *const c_char) }
                .to_string_lossy()
                .to_string();
            return Err(format!("pcap_findalldevs failed: {error}"));
        }

        let mut devices = Vec::new();
        let mut current = all_devices;
        while !current.is_null() {
            let device = unsafe { &*current };
            let name = if device.name.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(device.name) }
                    .to_string_lossy()
                    .to_string()
            };
            let description = if device.description.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(device.description) }
                    .to_string_lossy()
                    .to_string()
            };

            let is_loopback = (device.flags & PCAP_IF_LOOPBACK) != 0;
            let has_addresses = !device.addresses.is_null();
            let connection_status = device.flags & PCAP_IF_CONNECTION_STATUS;
            let status_known = matches!(connection_status, 0x0010 | PCAP_IF_CONNECTION_STATUS_DISCONNECTED);
            let disconnected = status_known && connection_status == PCAP_IF_CONNECTION_STATUS_DISCONNECTED;
            let name_lower = name.to_ascii_lowercase();
            let desc_lower = description.to_ascii_lowercase();
            let special = is_loopback || ["tunnel", "happ", "xray", "sing", "v2ray", "clash", "wintun", "wireguard", "tap", "openvpn", "outline", "exitlag", "gearup", "noping", "lagofast", "mudfish", "radmin", "zerotier", "hamachi"]
                .iter().any(|needle| name_lower.contains(needle) || desc_lower.contains(needle));
            if disconnected || (!is_loopback && !has_addresses && !special) {
                current = device.next;
                continue;
            }
            devices.push(DeviceInfo {
                name,
                description,
                has_addresses,
                is_loopback,
            });

            current = device.next;
        }

        unsafe { (self.freealldevs)(all_devices) };
        Ok(devices)
    }

    fn open_live_handle(&self, device_name: &str, timeout_ms: i32) -> Result<PcapT, String> {
        let device_name = CString::new(device_name)
            .map_err(|error| format!("Invalid device name for pcap_open_live: {error}"))?;
        let mut errbuf = [0u8; 256];

        let handle = unsafe {
            (self.open_live)(
                device_name.as_ptr(),
                65_535,
                1,
                timeout_ms,
                errbuf.as_mut_ptr() as *mut c_char,
            )
        };

        if handle.is_null() {
            let error = unsafe { CStr::from_ptr(errbuf.as_ptr() as *const c_char) }
                .to_string_lossy()
                .to_string();
            return Err(format!("pcap_open_live failed: {error}"));
        }

        Ok(handle)
    }
}

unsafe impl Send for NpcapLib {}
unsafe impl Sync for NpcapLib {}

#[derive(Clone)]
pub struct PcapCapturer {
    channel: Channel<CapturedPacket>,
    logger: Arc<AppLogger>,
    running: Arc<AtomicBool>,
    detector_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    capture_threads: Arc<Mutex<Vec<(Arc<AtomicBool>, JoinHandle<()>)>>>,
    target_device: Arc<RwLock<Option<String>>>,
    target_port: Arc<RwLock<Option<String>>>,
    last_target_packet: Arc<Mutex<Option<Instant>>>,
    detection_interval: Duration,
    detection_timeout: Duration,
}

impl PcapCapturer {
    pub fn new(channel: Channel<CapturedPacket>, logger: Arc<AppLogger>) -> Self {
        Self {
            channel,
            logger,
            running: Arc::new(AtomicBool::new(false)),
            detector_thread: Arc::new(Mutex::new(None)),
            capture_threads: Arc::new(Mutex::new(Vec::new())),
            target_device: Arc::new(RwLock::new(None)),
            target_port: Arc::new(RwLock::new(None)),
            last_target_packet: Arc::new(Mutex::new(None)),
            detection_interval: Duration::from_secs(1),
            detection_timeout: Duration::from_secs(3),
        }
    }

    pub fn run(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let channel = self.channel.clone();
        let logger = Arc::clone(&self.logger);
        let running = Arc::clone(&self.running);
        let capture_threads = Arc::clone(&self.capture_threads);
        let target_device = Arc::clone(&self.target_device);
        let target_port = Arc::clone(&self.target_port);
        let last_target_packet = Arc::clone(&self.last_target_packet);
        let detection_interval = self.detection_interval;
        let detection_timeout = self.detection_timeout;

        let detector_handle = thread::spawn(move || {
            let npcap = match NpcapLib::load() {
                Ok(api) => Arc::new(api),
                Err(error) => {
                    logger.error(format!("Npcap initialization failed: {error}"));
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            };
            let mut last_device_inventory = String::new();
            let mut last_detected_target = String::new();

            while running.load(Ordering::SeqCst) {
                if let Some(active_device) = target_device.read().unwrap().clone() {
                    sleep_while_running(&running, Duration::from_secs(1));
                    if last_target_packet.lock().unwrap().is_some_and(|last| last.elapsed() < Duration::from_secs(8)) {
                        continue;
                    }
                    logger.info(format!("capture target stale: device={} flow={} idle_for>=8s; rescanning", active_device, target_port.read().unwrap().as_deref().unwrap_or("--")));
                    *target_device.write().unwrap() = None;
                    *target_port.write().unwrap() = None;
                    *last_target_packet.lock().unwrap() = None;
                    stop_capture_threads(&capture_threads);
                }
                // 1. find magic devices
                let devices = match npcap.find_all_devices() {
                    Ok(devices) => devices,
                    Err(error) => {
                        logger.error(format!("Failed to enumerate capture devices: {error}"));
                        thread::sleep(detection_interval);
                        continue;
                    }
                };

                let devices = prioritize_devices(devices);
                let inventory = format_device_inventory(&devices);
                if inventory != last_device_inventory {
                    if devices.is_empty() {
                        logger.info("capture device: none");
                    } else {
                        for device_line in &devices {
                            logger.info(format!(
                                "capture device: {} | desc={} | has_addresses={} | loopback={} | virtual={} | priority={}",
                                device_line.name,
                                if device_line.description.is_empty() {
                                    "--"
                                } else {
                                    device_line.description.as_str()
                                },
                                device_line.has_addresses,
                                device_line.is_loopback,
                                device_line.is_virtual(),
                                device_line.priority_label()
                            ));
                        }
                    }
                    last_device_inventory = inventory;
                }
                let detections = inspect_devices_for_magic(Arc::clone(&npcap), &devices, detection_timeout, &running);
                for detection in &detections {
                    logger.info(format!("capture scan: device={} flow={} hits={} eligible={}", detection.device_name, detection.flow, detection.hits, detection.hits >= MAGIC_MIN_HITS));
                }
                for device in &devices {
                    if !detections.iter().any(|d| d.device_name == device.name) {
                        logger.info(format!("capture scan: device={} flow=none hits=0 eligible=false", device.name));
                    }
                }
                let detected_target = choose_target(&devices, &detections).map(|d| CaptureTarget {
                    device_name: d.device_name.clone(),
                    target_port: Some(d.flow.clone()),
                });

                if let Some(target) = detected_target {
                    let detected_signature = format!(
                        "{}@{}",
                        target.device_name,
                        target.target_port.as_deref().unwrap_or("--")
                    );
                    if detected_signature != last_detected_target {
                        logger.info(format!(
                            "capture target detected: device={} flow={} reason={}",
                            target.device_name,
                            target.target_port.as_deref().unwrap_or("--"),
                            devices.iter().find(|device| device.name == target.device_name).map(|device| if device.is_virtual() { "plaintext game flow on loopback/tunnel; preferred over physical adapter" } else { "game flow on physical adapter; no preferred virtual capture observed" }).unwrap_or("selected highest-priority qualifying flow")
                        ));
                        last_detected_target = detected_signature;
                    }

                    let previous_device = target_device.read().unwrap().clone();
                    let should_restart = previous_device.as_deref()
                        != Some(target.device_name.as_str())
                        || capture_threads.lock().unwrap().is_empty();

                    *target_device.write().unwrap() = Some(target.device_name.clone());
                    *target_port.write().unwrap() = target.target_port.clone();

                    if should_restart {
                        stop_capture_threads(&capture_threads);
                        *last_target_packet.lock().unwrap() = Some(Instant::now());
                        start_capture_thread(
                            Arc::clone(&npcap),
                            target.device_name.clone(),
                            channel.clone(),
                            Arc::clone(&running),
                            Arc::clone(&capture_threads),
                            Arc::clone(&last_target_packet),
                            target.target_port.clone(),
                        );
                    }
                    *last_target_packet.lock().unwrap() = Some(Instant::now());
                    continue;
                }
                *target_device.write().unwrap() = None;
                *target_port.write().unwrap() = None;
                *last_target_packet.lock().unwrap() = None;
                sleep_while_running(&running, detection_interval);
            }

            stop_capture_threads(&capture_threads);
        });

        *self.detector_thread.lock().unwrap() = Some(detector_handle);
    }

    pub fn start(&self) -> Result<(), String> {
        self.stop();
        NpcapLib::load()?;
        self.run();
        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.detector_thread.lock().unwrap().take() {
            let _ = handle.join();
        }
        stop_capture_threads(&self.capture_threads);
        *self.target_device.write().unwrap() = None;
        *self.target_port.write().unwrap() = None;
        *self.last_target_packet.lock().unwrap() = None;
    }

    pub fn target_device(&self) -> Option<String> {
        self.target_device.read().unwrap().clone()
    }

    pub fn target_port(&self) -> Option<String> {
        self.target_port.read().unwrap().clone()
    }

    pub fn channel(&self) -> Channel<CapturedPacket> {
        self.channel.clone()
    }
}

impl Drop for PcapCapturer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn check_npcap_available() -> Result<(), String> {
    NpcapLib::load().map(|_| ())
}

/// A stricter check than [`check_npcap_available`], for the startup gate.
///
/// Loading `wpcap.dll` only proves a file is on the search path. It keeps
/// succeeding after the Npcap service is stopped or its driver is uninstalled
/// from under it, so a gate built on that would let a user into an app that can
/// never see a packet. Enumerating adapters actually reaches the driver.
///
/// Returns how many non-loopback adapters are visible.
pub fn probe_npcap() -> Result<usize, String> {
    let npcap = NpcapLib::load().map_err(|_| {
        "Npcap is not installed, or was installed without WinPcap API-compatible Mode."
            .to_string()
    })?;

    let devices = npcap.find_all_devices().map_err(|error| {
        format!("Npcap is installed but would not list adapters ({error}).")
    })?;

    let usable = devices.iter().filter(|device| !device.is_loopback).count();
    if usable == 0 {
        return Err(
            "Npcap answered but reported no network adapters. Its driver service may be stopped."
                .to_string(),
        );
    }

    Ok(usable)
}

fn format_device_inventory(devices: &[DeviceInfo]) -> String {
    if devices.is_empty() {
        return "none".to_string();
    }

    devices
        .iter()
        .map(|device| {
            format!(
                "{}|{}|{}|{}|{}",
                device.name,
                device.description,
                device.has_addresses,
                device.is_loopback,
                device.is_virtual()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn prioritize_devices(mut devices: Vec<DeviceInfo>) -> Vec<DeviceInfo> {
    devices.retain(|device| device.is_loopback || device.has_addresses || device.is_virtual());
    devices.sort_by_key(|device| {
        let loopback_priority = if device.is_loopback { 0 } else if device.is_virtual() { 1 } else { 2 };
        let name = device.label().to_ascii_lowercase();
        (loopback_priority, name)
    });
    devices
}

#[derive(Debug, Clone)]
struct DeviceDetection { device_name: String, flow: String, hits: u32 }

/// A pcap handle owned by exactly one worker thread (opened here, read and closed only by that worker).
struct WorkerHandle(PcapT);
unsafe impl Send for WorkerHandle {}
impl WorkerHandle {
    // a method call makes the closure capture the whole wrapper, not the raw pointer field
    fn get(&self) -> PcapT { self.0 }
}

fn inspect_devices_for_magic(
    npcap: Arc<NpcapLib>,
    devices: &[DeviceInfo],
    timeout: Duration,
    running: &Arc<AtomicBool>,
)-> Vec<DeviceDetection> {
    let opened: Vec<_> = devices.iter().filter_map(|device| {
        match npcap.open_live_handle(&device.name, 100) {
            Ok(handle) => Some((device.name.clone(), handle)),
            Err(error) => { eprintln!("capture open failed device={} error={}", device.name, error); None }
        }
    }).collect();
    let results = Arc::new(Mutex::new(Vec::<DeviceDetection>::new()));
    let deadline = Instant::now() + timeout;
    let mut workers = Vec::new();
    for (device_name, handle) in opened {
        let handle = WorkerHandle(handle);
        let api = Arc::clone(&npcap);
        let out = Arc::clone(&results);
        let running = Arc::clone(running);
        workers.push(thread::spawn(move || {
            let mut hits: std::collections::HashMap<(u16,u16),u32> = std::collections::HashMap::new();
            while running.load(Ordering::SeqCst) && Instant::now() < deadline {
                if let CaptureRead::Packet(packet) = next_captured_packet(api.as_ref(), handle.get()) {
                    if packet.data.starts_with(&MAGIC_PATTERN) {
                        let flow = if packet.src_port <= packet.dst_port { (packet.src_port, packet.dst_port) } else { (packet.dst_port, packet.src_port) };
                        *hits.entry(flow).or_default() += 1;
                    }
                }
            }
            unsafe { (api.close)(handle.get()) };
            let mut out = out.lock().unwrap();
            out.extend(hits.into_iter().map(|((a,b),hits)| DeviceDetection { device_name: device_name.clone(), flow: format!("{a}-{b}"), hits }));
        }));
    }
    for worker in workers { let _ = worker.join(); }
    let detections = results.lock().unwrap().clone();
    detections
}

fn choose_target<'a>(devices: &'a [DeviceInfo], detections: &'a [DeviceDetection]) -> Option<&'a DeviceDetection> {
    detections.iter().filter(|d| d.hits >= MAGIC_MIN_HITS).max_by(|a,b| {
        let priority = |d: &DeviceDetection| devices.iter().find(|device| device.name == d.device_name).map(|device| if device.is_loopback { 2 } else if device.is_virtual() { 1 } else { 0 }).unwrap_or(0);
        priority(a).cmp(&priority(b)).then_with(|| a.hits.cmp(&b.hits))
    })
}

fn start_capture_thread(
    npcap: Arc<NpcapLib>,
    device_name: String,
    channel: Channel<CapturedPacket>,
    running: Arc<AtomicBool>,
    capture_threads: Arc<Mutex<Vec<(Arc<AtomicBool>, JoinHandle<()>)>>>,
    last_target_packet: Arc<Mutex<Option<Instant>>>,
    target_port: Option<String>,
) {
    let capture_running = Arc::new(AtomicBool::new(true));
    let thread_running = Arc::clone(&capture_running);
    let handle = thread::spawn(move || {
        let capture_handle = match npcap.open_live_handle(&device_name, 100) {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("Failed to open capture on {device_name}: {error}");
                return;
            }
        };

        while running.load(Ordering::SeqCst) && thread_running.load(Ordering::SeqCst) {
            match next_captured_packet(&npcap, capture_handle) {
                CaptureRead::Packet(packet) => {
                    let matches_target = target_port.as_deref().map_or(true, |flow| {
                        let (a,b) = if packet.src_port <= packet.dst_port {(packet.src_port,packet.dst_port)} else {(packet.dst_port,packet.src_port)};
                        flow == format!("{a}-{b}")
                    });
                    if matches_target { *last_target_packet.lock().unwrap() = Some(Instant::now()); }
                    let _ = channel.try_send(packet);
                }
                CaptureRead::Timeout => continue,
                CaptureRead::End => break,
                CaptureRead::Error => break,
            }
        }

        unsafe { (npcap.close)(capture_handle) };
    });

    capture_threads.lock().unwrap().push((capture_running, handle));
}

fn stop_capture_threads(capture_threads: &Arc<Mutex<Vec<(Arc<AtomicBool>, JoinHandle<()>)>>>) {
    let handles = {
        let mut guard = capture_threads.lock().unwrap();
        std::mem::take(&mut *guard)
    };

    for (stop, handle) in handles {
        stop.store(false, Ordering::SeqCst);
        let _ = handle.join();
    }
}

enum CaptureRead {
    Packet(CapturedPacket),
    Timeout,
    End,
    Error,
}

fn next_captured_packet(npcap: &NpcapLib, handle: PcapT) -> CaptureRead {
    let mut header: *mut PcapPkthdr = ptr::null_mut();
    let mut data: *const u8 = ptr::null();

    let ret = unsafe { (npcap.next_ex)(handle, &mut header, &mut data) };
    match ret {
        1 => {
            let header = unsafe { &*header };
            let frame = unsafe { std::slice::from_raw_parts(data, header.caplen as usize) };
            parse_captured_packet(frame, header)
                .map(CaptureRead::Packet)
                .unwrap_or(CaptureRead::Timeout)
        }
        0 => CaptureRead::Timeout,
        -2 => CaptureRead::End,
        _ => CaptureRead::Error,
    }
}

fn parse_captured_packet(frame: &[u8], header: &PcapPkthdr) -> Option<CapturedPacket> {
    let ip_offset = detect_ip_offset(frame)?;
    if frame.len() < ip_offset + 20 {
        return None;
    }

    let ip_header = &frame[ip_offset..];
    if (ip_header[0] >> 4) != 4 {
        return None;
    }

    let ip_header_len = ((ip_header[0] & 0x0F) as usize) * 4;
    if ip_header[9] != 6 {
        return None;
    }

    let tcp_offset = ip_offset + ip_header_len;
    if frame.len() < tcp_offset + 20 {
        return None;
    }

    let tcp_header = &frame[tcp_offset..];
    let src_port = u16::from_be_bytes([tcp_header[0], tcp_header[1]]);
    let dst_port = u16::from_be_bytes([tcp_header[2], tcp_header[3]]);
    let sequence = u32::from_be_bytes([tcp_header[4], tcp_header[5], tcp_header[6], tcp_header[7]]);
    let tcp_header_len = ((tcp_header[12] >> 4) as usize) * 4;
    let payload_offset = tcp_offset + tcp_header_len;
    let ip_total_len =
        usize::from(u16::from_be_bytes([ip_header[2], ip_header[3]])).min(frame.len() - ip_offset);
    let payload_end = ip_offset + ip_total_len;
    if payload_offset >= payload_end {
        return None;
    }

    let payload = &frame[payload_offset..payload_end];
    if payload.is_empty() {
        return None;
    }

    Some(CapturedPacket {
        src_ip: [ip_header[12], ip_header[13], ip_header[14], ip_header[15]],
        src_port,
        dst_ip: [ip_header[16], ip_header[17], ip_header[18], ip_header[19]],
        dst_port,
        sequence,
        data: payload.to_vec(),
        captured_at: pcap_header_timestamp_seconds(header),
    })
}

fn detect_ip_offset(frame: &[u8]) -> Option<usize> {
    if frame.len() >= 14 {
        let ether_type = u16::from_be_bytes([frame[12], frame[13]]);
        if ether_type == 0x0800 {
            return Some(14);
        }
        if frame[0] == 2 && frame[1] == 0 && frame[2] == 0 && frame[3] == 0 {
            return Some(4);
        }
    }

    if !frame.is_empty() && (frame[0] >> 4) == 4 {
        return Some(0);
    }

    None
}

fn pcap_header_timestamp_seconds(header: &PcapPkthdr) -> f64 {
    let from_pcap = (header.ts_sec as f64) + (header.ts_usec as f64 / 1_000_000.0);
    if from_pcap > 1_000_000_000.0 {
        from_pcap
    } else {
        current_timestamp_seconds()
    }
}

fn current_timestamp_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

fn sleep_while_running(running: &Arc<AtomicBool>, duration: Duration) {
    let deadline = Instant::now() + duration;
    while running.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str, description: &str, loopback: bool, addresses: bool) -> DeviceInfo {
        DeviceInfo { name: name.into(), description: description.into(), is_loopback: loopback, has_addresses: addresses }
    }

    fn ipv4_tcp_frame(prefix: &[u8], src_port: u16, payload: &[u8]) -> Vec<u8> {
        let mut ip = vec![0u8; 40 + payload.len()];
        ip[0] = 0x45;
        let total = ip.len() as u16;
        ip[2..4].copy_from_slice(&total.to_be_bytes());
        ip[9] = 6;
        ip[12..16].copy_from_slice(&[172, 19, 0, 1]);
        ip[16..20].copy_from_slice(&[193, 202, 112, 113]);
        ip[20..22].copy_from_slice(&src_port.to_be_bytes());
        ip[22..24].copy_from_slice(&13328u16.to_be_bytes());
        ip[32] = 0x50;
        ip[40..].copy_from_slice(payload);
        let mut frame = prefix.to_vec();
        frame.extend(ip);
        frame
    }

    fn parse_frame(frame: &[u8]) -> CapturedPacket {
        let header = PcapPkthdr { ts_sec: 1_800_000_000, ts_usec: 0, caplen: frame.len() as u32, len: frame.len() as u32 };
        parse_captured_packet(frame, &header).expect("synthetic TCP frame parses")
    }

    #[test]
    fn parses_game_magic_from_raw_ip_ethernet_and_loopback_null_frames() {
        for prefix in [&[][..], &[0, 0, 0, 2][..], &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 8, 0][..]] {
            let frame = ipv4_tcp_frame(prefix, 54745, &[0x0e, 0x00, 0x36, 1]);
            let packet = parse_frame(&frame);
            assert!(packet.data.starts_with(&MAGIC_PATTERN));
            assert_eq!((packet.src_port, packet.dst_port), (54745, 13328));
        }
    }

    #[test]
    fn https_noise_does_not_qualify_and_shared_flow_prefers_tunnel() {
        let wan_without_address = dev("wan-miniport", "WAN Miniport", false, false);
        let physical = dev("\\Device\\Physical", "Wi-Fi", false, true);
        let tunnel = dev("\\Device\\Happ", "Happ Tunnel", false, true);
        let devices = prioritize_devices(vec![wan_without_address, physical.clone(), tunnel.clone()]);
        assert_eq!(devices.len(), 2, "addressless non-loopback adapters are skipped");
        assert!(devices[0].is_virtual());
        let packets = [
            CapturedPacket { src_ip: [1,2,3,4], src_port: 50000, dst_ip: [5,6,7,8], dst_port: 443, sequence: 0, data: vec![0x16,3,1,0,0x0e,0,0x36], captured_at: 0.0 },
            CapturedPacket { src_ip: [172,19,0,1], src_port: 54745, dst_ip: [193,202,112,113], dst_port: 13328, sequence: 1, data: vec![0x0e,0,0x36], captured_at: 0.0 },
        ];
        assert!(!packets[0].data.starts_with(&MAGIC_PATTERN));
        let chosen = choose_target(&[physical, tunnel], &[
            DeviceDetection { device_name: "\\Device\\Physical".into(), flow: "54745-13328".into(), hits: MAGIC_MIN_HITS },
            DeviceDetection { device_name: "\\Device\\Happ".into(), flow: "54745-13328".into(), hits: MAGIC_MIN_HITS },
            DeviceDetection { device_name: "\\Device\\Happ".into(), flow: "50000-443".into(), hits: 0 },
        ]).unwrap();
        assert_eq!(chosen.device_name, "\\Device\\Happ");
        assert_eq!(chosen.flow, "54745-13328");
    }
}
