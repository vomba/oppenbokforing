//! OS setup is mandatory and verified BEFORE input read/parse. A relative AS
//! limit bounds additional VM reservation, not RSS or pages in existing mappings.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn install() -> Result<(), ()> {
    #[cfg(target_os = "macos")]
    type Resource = libc::c_int;
    #[cfg(target_os = "linux")]
    type Resource = libc::__rlimit_resource_t;
    fn set(resource: Resource, value: libc::rlim_t) -> Result<(), ()> {
        let requested = libc::rlimit {
            rlim_cur: value,
            rlim_max: value,
        };
        let mut actual = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: valid pointers to ABI-provided libc structs. No pre_exec/fork hooks.
        if unsafe { libc::setrlimit(resource, &requested) } != 0
            || unsafe { libc::getrlimit(resource, &mut actual) } != 0
            || actual.rlim_cur != value
            || actual.rlim_max != value
        {
            return Err(());
        }
        Ok(())
    }
    set(libc::RLIMIT_CORE, 0)?;
    set(libc::RLIMIT_CPU, 2)?;
    let vm = virtual_size()?;
    let ceiling = vm.checked_add(256 * 1024 * 1024).ok_or(())?;
    let ceiling = libc::rlim_t::try_from(ceiling).map_err(|_| ())?;
    set(libc::RLIMIT_AS, ceiling)
}
#[cfg(target_os = "macos")]
fn virtual_size() -> Result<u64, ()> {
    // SAFETY: SDK's MACH_TASK_BASIC_INFO count and layout; task is this exec child.
    unsafe {
        let mut info: libc::mach_task_basic_info = std::mem::zeroed();
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
        // libc deprecates the exported task-self symbol in favor of a new
        // dependency. Declare that same SDK symbol using libc's ABI types instead.
        extern "C" {
            static mach_task_self_: libc::mach_port_t;
        }
        let result = libc::task_info(
            mach_task_self_,
            libc::MACH_TASK_BASIC_INFO,
            &mut info as *mut _ as *mut libc::integer_t,
            &mut count,
        );
        if result != libc::KERN_SUCCESS || count != libc::MACH_TASK_BASIC_INFO_COUNT {
            return Err(());
        }
        Ok(info.virtual_size)
    }
}
#[cfg(target_os = "linux")]
fn virtual_size() -> Result<u64, ()> {
    use std::io::Read;
    let mut status = String::new();
    std::fs::File::open("/proc/self/status")
        .map_err(|_| ())?
        .take(16 * 1024)
        .read_to_string(&mut status)
        .map_err(|_| ())?;
    let value = status
        .lines()
        .find_map(|s| s.strip_prefix("VmSize:"))
        .ok_or(())?;
    let mut words = value.split_whitespace();
    let kib: u64 = words.next().ok_or(())?.parse().map_err(|_| ())?;
    if words.next() != Some("kB") {
        return Err(());
    }
    kib.checked_mul(1024).ok_or(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn install() -> Result<(), ()> {
    Err(())
} // No silently unbounded fallback.
