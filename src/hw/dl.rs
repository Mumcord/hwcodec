//! Loads GPU driver libraries at runtime (dlopen / LoadLibrary). The drivers are never linked, so
//! a program starts on machines without them; a missing library only makes that encoder
//! unavailable.

use libloading::Library;

use super::{HardwareEncoder, Probe};
use crate::Result;
use crate::detect::UnavailableReason;

#[cfg(all(feature = "nvenc", windows))]
pub(super) const NVENC: &[&str] = &["nvEncodeAPI64.dll"];
#[cfg(all(feature = "nvenc", not(windows)))]
pub(super) const NVENC: &[&str] = &["libnvidia-encode.so.1"];

#[cfg(all(feature = "amf", windows))]
pub(super) const AMF: &[&str] = &["amfrt64.dll"];
#[cfg(all(feature = "amf", not(windows)))]
pub(super) const AMF: &[&str] = &["libamfrt64.so.1"];

#[cfg(all(feature = "vaapi", target_os = "linux"))]
pub(super) const VA: &[&str] = &["libva.so.2"];
#[cfg(all(feature = "vaapi", target_os = "linux"))]
pub(super) const VA_DRM: &[&str] = &["libva-drm.so.2"];

/// Loads the first of `names` that exists, or reports `encoder` as unavailable because its
/// driver was not found.
pub(super) fn open(encoder: HardwareEncoder, names: &[&str], probe: &mut Probe) -> Result<Library> {
    let mut failures = Vec::new();
    for name in names {
        match load(name) {
            Ok(library) => return Ok(library),
            Err(err) => {
                // Unix loaders already name the library in their error; Windows ones do not.
                let err = err.to_string();
                failures.push(if err.contains(name) {
                    err
                } else {
                    format!("{name}: {err}")
                });
            }
        }
    }
    probe.reason = Some(UnavailableReason::DriverNotFound);
    Err(encoder.unavailable(format!(
        "driver library not loaded ({})",
        failures.join("; ")
    )))
}

/// Looks up the function or variable `name` in `library`.
///
/// # Safety
/// `T` must be the exact type of the symbol (usually an `unsafe extern "C" fn` pointer).
#[cfg(any(test, feature = "nvenc", feature = "amf"))]
pub(super) unsafe fn symbol<T: Copy>(
    encoder: HardwareEncoder,
    library: &Library,
    name: &str,
) -> Result<T> {
    // SAFETY: the caller guarantees that `T` matches the symbol.
    unsafe { library.get::<T>(name.as_bytes()) }
        .map(|symbol| *symbol)
        .map_err(|err| encoder.unavailable(format!("driver lacks {name}: {err}")))
}

#[cfg(windows)]
fn load(name: &str) -> std::result::Result<Library, libloading::Error> {
    use libloading::os::windows::{LOAD_LIBRARY_SEARCH_SYSTEM32, Library as WindowsLibrary};
    // Drivers install their DLLs into System32; searching only there keeps a planted DLL next
    // to the app or in the working directory from being loaded instead.
    // SAFETY: loading runs the driver's initialisation code, which the system's own graphics
    // stack runs as well.
    unsafe { WindowsLibrary::load_with_flags(name, LOAD_LIBRARY_SEARCH_SYSTEM32) }.map(Into::into)
}

#[cfg(not(windows))]
fn load(name: &str) -> std::result::Result<Library, libloading::Error> {
    // SAFETY: loading runs the driver's initialisation code, which the system's own graphics
    // stack runs as well.
    unsafe { Library::new(name) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn a_missing_library_makes_the_encoder_unavailable() {
        let mut probe = Probe::default();
        let err = open(
            HardwareEncoder::Nvenc,
            &["hwcodec-no-such-driver-1", "hwcodec-no-such-driver-2"],
            &mut probe,
        )
        .expect_err("no such library");
        assert_eq!(probe.reason, Some(UnavailableReason::DriverNotFound));
        let Error::HardwareUnavailable { encoder, reason } = err else {
            panic!("unexpected error {err}");
        };
        assert_eq!(encoder, HardwareEncoder::Nvenc);
        assert!(reason.contains("hwcodec-no-such-driver-2"), "{reason}");
    }

    #[test]
    fn loads_a_system_library_and_finds_its_symbols() {
        #[cfg(windows)]
        let (names, function) = (["kernel32.dll"], "GetCurrentProcessId");
        #[cfg(not(windows))]
        let (names, function) = (["libc.so.6"], "getpid");
        let mut probe = Probe::default();
        let library = open(HardwareEncoder::Amf, &names, &mut probe).expect("system library");
        assert_eq!(probe.reason, None);
        // SAFETY: both functions take no arguments and return a 32-bit process id.
        let pid = unsafe {
            symbol::<unsafe extern "system" fn() -> u32>(HardwareEncoder::Amf, &library, function)
        }
        .expect("symbol");
        // SAFETY: see above.
        assert_eq!(unsafe { pid() }, std::process::id());
        // SAFETY: see above.
        let missing = unsafe {
            symbol::<unsafe extern "C" fn()>(
                HardwareEncoder::Amf,
                &library,
                "hwcodec_no_such_symbol",
            )
        };
        assert!(matches!(missing, Err(Error::HardwareUnavailable { .. })));
    }
}
