// Rust c_interface to libhackrf FFI bindings
// Copyright Adam Greig <adam@adamgreig.com> 2014
// Licensed under MIT license

#![allow(dead_code)]

use std::ffi::{c_int, c_uint, c_void};

mod ffi;

pub struct HackRFDevice {
    ptr: *mut ffi::hackrf_device,
}

impl Drop for HackRFDevice {
    #[inline(never)]
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                ffi::hackrf_close(self.ptr);
            }
        }
    }
}

pub struct HackRFError {
    errno: c_int,
    errstr: String,
}

impl std::fmt::Debug for HackRFError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "HackRF error: {} ({})", self.errstr, self.errno)
    }
}

fn hackrf_error(err: c_int) -> HackRFError {
    let s = unsafe {
        let ptr = ffi::hackrf_error_name(err);
        std::ffi::CStr::from_ptr(ptr)
    };
    HackRFError {
        errno: err as c_int,
        errstr: s.to_str().unwrap().to_string(),
    }
}

/// Initialise the HackRF library. Call this once at application startup.
pub fn init() -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_init() } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// De-initialise the HackRF library. Call this once at application
/// termination.
pub fn exit() -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_exit() } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Attempt to open a connected HackRF device.
pub fn open() -> Result<HackRFDevice, HackRFError> {
    let mut device: HackRFDevice = unsafe { std::mem::zeroed() };
    match unsafe { ffi::hackrf_open(&mut device.ptr) } {
        ffi::HACKRF_SUCCESS => Ok(device),
        err => Err(hackrf_error(err)),
    }
}

/// Close a connected HackRF device.
pub fn close(device: HackRFDevice) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_close(device.ptr) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// The library defines the C callback, which will itself call a closure
/// inside Rust after resolving memory stuff, so that users don't need to
/// write unsafe code.
extern "C" fn rx_cb(transfer: *mut ffi::hackrf_transfer) -> c_int {
    unsafe {
        let data = &*transfer;
        let buffer: &[u8] = std::slice::from_raw_parts(data.buffer, data.buffer_length as usize);
        let cb: &mut &mut dyn FnMut(&[u8]) -> bool = std::mem::transmute(data.rx_ctx);

        match (**cb)(buffer) {
            true => 0 as c_int,
            false => 1 as c_int,
        }
    }
}

/// The library defines the C callback, which will itself call a closure
/// inside Rust after resolving memory stuff, so that users don't need to
/// write unsafe code.
extern "C" fn tx_cb(transfer: *mut ffi::hackrf_transfer) -> c_int {
    unsafe {
        let data = &*transfer;
        let buffer: &mut [u8] = std::slice::from_mut(&mut *data.buffer);

        let cb: *mut &mut dyn FnMut(&mut [u8]) -> bool = std::mem::transmute(data.rx_ctx);

        match (*cb)(buffer) {
            true => 0 as c_int,
            false => 1 as c_int,
        }
    }
}

/// Begin RX stream.
/// `callback` is a borrowed reference to a closure like:
///     callback(buffer: &[u8]) -> bool
/// which is given `buffer`, the RX buffer, and returns `true` if it should
/// continue receiving data or `false` to stop. It may be called a few times
/// after returning `false` while the system catches up.
pub fn start_rx(
    device: &mut HackRFDevice,
    callback: &mut dyn FnMut(&[u8]) -> bool,
) -> Result<(), HackRFError> {
    let boxed = Box::new(callback);
    let reference = Box::leak(boxed);
    let ctx = unsafe { std::mem::transmute(reference as *mut &mut dyn FnMut(&[u8]) -> bool) };
    match unsafe { ffi::hackrf_start_rx(device.ptr, rx_cb, ctx) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Stop RX stream
pub fn stop_rx(device: &mut HackRFDevice) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_stop_rx(device.ptr) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Begin TX stream
/// `callback` is a borrowed reference to a closure like:
///     callback(buffer: &mut[u8]) -> bool
/// which is given `buffer`, the TX buffer, and returns `true` if it should
/// continue sending data or `false` to stop. It may be called a few times
/// after returning `false` while the system catches up.
/// Modify the TX slice at leisure and it will be transmitted over the radio.
pub fn start_tx(
    device: &mut HackRFDevice,
    callback: &mut impl FnMut(&mut [u8]) -> bool,
) -> Result<(), HackRFError> {
    let ctx = (callback as *mut _) as *mut std::ffi::c_void;
    match unsafe { ffi::hackrf_start_tx(device.ptr, tx_cb, ctx) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Stop TX stream
pub fn stop_tx(device: &mut HackRFDevice) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_stop_tx(device.ptr) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Check if a HackRF device is currently streaming data.
/// Returns true if so, false if stopped due to streaming finishing
/// or exit being called, and an error if not streaming due to error.
pub fn is_streaming(device: &mut HackRFDevice) -> Result<bool, HackRFError> {
    match unsafe { ffi::hackrf_is_streaming(device.ptr) } {
        ffi::HACKRF_TRUE => Ok(true),
        ffi::HACKRF_ERROR_STREAMING_STOPPED | ffi::HACKRF_ERROR_STREAMING_EXIT_CALLED => Ok(false),
        err => Err(hackrf_error(err)),
    }
}

/// Set the HackRF baseband filter bandwidth, in Hz.
/// See also `compute_baseband_filter_bw` and
/// `compute_baseband_filter_bw_round_down_lt`.
pub fn set_baseband_filter_bandwidth(
    device: &mut HackRFDevice,
    bandwidth_hz: c_uint,
) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_set_baseband_filter_bandwidth(device.ptr, bandwidth_hz as u32) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Read the board ID. Returns a tuple of the numeric ID and a corresponding
/// String. This is the product identifier, not a serial number.
pub fn board_id_read(device: &mut HackRFDevice) -> Result<(c_int, String), HackRFError> {
    let mut id: u8 = ffi::BOARD_ID_INVALID;
    match unsafe { ffi::hackrf_board_id_read(device.ptr, &mut id) } {
        ffi::HACKRF_SUCCESS => {
            let s = unsafe {
                let ptr = ffi::hackrf_board_id_name(id as u8);
                std::ffi::CStr::from_ptr(ptr)
            };
            Ok((id as c_int, s.to_str().unwrap().to_string()))
        }
        err => Err(hackrf_error(err)),
    }
}

/// Read the board's firmware version string.
pub fn version_string_read(device: &mut HackRFDevice) -> Result<String, HackRFError> {
    let mut buf = [0; 127];
    match unsafe { ffi::hackrf_version_string_read(device.ptr, buf.as_mut_ptr(), 127) } {
        ffi::HACKRF_SUCCESS => {
            let s = unsafe { std::str::from_utf8(std::mem::transmute(buf.as_slice())) };
            Ok(String::from(s.unwrap()))
        }
        err => Err(hackrf_error(err)),
    }
}

/// Read the part ID and serial number
pub fn board_partid_serialno_read(
    device: &mut HackRFDevice,
) -> Result<([u32; 2], [u32; 4]), HackRFError> {
    let mut serial: ffi::read_partid_serialno_t = unsafe { std::mem::zeroed() };
    match unsafe { ffi::hackrf_board_partid_serialno_read(device.ptr, &mut serial) } {
        ffi::HACKRF_SUCCESS => Ok((serial.part_id, serial.serial_no)),
        err => Err(hackrf_error(err)),
    }
}

/// Set HackRF frequency
pub fn set_freq(device: &mut HackRFDevice, freq_hz: u64) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_set_freq(device.ptr, freq_hz) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

#[derive(Copy, Clone)]
pub enum RFPathFilter {
    Bypass,
    LowPass,
    HighPass,
}

/// Set HackRF frequency, specifying IF and LO and filters separately.
/// `path` may be `RFPathFilter::Bypass`, `LowPass` or `HighPass`.
pub fn set_freq_explicit(
    device: &mut HackRFDevice,
    if_freq_hz: u64,
    lo_freq_hz: u64,
    path: RFPathFilter,
) -> Result<(), HackRFError> {
    let c_path = match path {
        RFPathFilter::Bypass => ffi::RF_PATH_FILTER_BYPASS,
        RFPathFilter::LowPass => ffi::RF_PATH_FILTER_LOW_PASS,
        RFPathFilter::HighPass => ffi::RF_PATH_FILTER_HIGH_PASS,
    };
    match unsafe { ffi::hackrf_set_freq_explicit(device.ptr, if_freq_hz, lo_freq_hz, c_path) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set HackRF sample rate, specifying c_integer frequency and divider
/// Preferred rates are 8, 10, 12.5, 16 and 20MHz
pub fn set_sample_rate_manual(
    device: &mut HackRFDevice,
    freq_hz: u32,
    divider: u32,
) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_set_sample_rate_manual(device.ptr, freq_hz, divider) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set HackRF sample rate, specifying frequency as a double float
/// Preferred rates are 8, 10, 12.5, 16 and 20MHz
pub fn set_sample_rate(device: &mut HackRFDevice, freq_hz: f64) -> Result<(), HackRFError> {
    match unsafe { ffi::hackrf_set_sample_rate(device.ptr, freq_hz) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set HackRF external amplifier on or off
pub fn set_amp_enable(device: &mut HackRFDevice, on: bool) -> Result<(), HackRFError> {
    let value = match on {
        false => 0u8,
        true => 1,
    };
    match unsafe { ffi::hackrf_set_amp_enable(device.ptr, value) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set LNA gain, 0-40 in steps of 8dB
pub fn set_lna_gain(device: &mut HackRFDevice, gain: u32) -> Result<(), HackRFError> {
    assert!(gain <= 40);
    match unsafe { ffi::hackrf_set_lna_gain(device.ptr, gain) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set VGA gain, 0-62 in steps of 2dB
pub fn set_vga_gain(device: &mut HackRFDevice, gain: u32) -> Result<(), HackRFError> {
    assert!(gain <= 62);
    match unsafe { ffi::hackrf_set_vga_gain(device.ptr, gain) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set TXVGA gain, 0-47 in steps of 1dB
pub fn set_txvga_gain(device: &mut HackRFDevice, gain: u32) -> Result<(), HackRFError> {
    assert!(gain <= 47);
    match unsafe { ffi::hackrf_set_txvga_gain(device.ptr, gain) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Set antenna port power on/off
pub fn set_antenna_enable(device: &mut HackRFDevice, on: bool) -> Result<(), HackRFError> {
    let value = match on {
        false => 0u8,
        true => 1,
    };
    match unsafe { ffi::hackrf_set_antenna_enable(device.ptr, value) } {
        ffi::HACKRF_SUCCESS => Ok(()),
        err => Err(hackrf_error(err)),
    }
}

/// Compute nearest frequency for bandwidth filter (manual filter)
pub fn compute_baseband_filter_bw_round_down_lt(bandwidth_hz: u32) -> u32 {
    unsafe { ffi::hackrf_compute_baseband_filter_bw_round_down_lt(bandwidth_hz) }
}

/// Compute best default value for bandwidth filter depending on sample rate
pub fn compute_baseband_filter_bw(bandwidth_hz: u32) -> u32 {
    unsafe { ffi::hackrf_compute_baseband_filter_bw(bandwidth_hz) }
}
